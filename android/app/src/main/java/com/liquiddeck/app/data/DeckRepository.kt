package com.liquiddeck.app.data

import android.util.Log
import java.net.URLEncoder
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import okhttp3.WebSocket
import okhttp3.WebSocketListener

private val JSON = "application/json; charset=utf-8".toMediaType()
private const val TAG = "LiquidDeck"

/**
 * Owns the WebSocket to the PC agent, the HTTP polling fallback, and the
 * request/response matching for actions.
 */
class DeckRepository {
    private val json = Json {
        ignoreUnknownKeys = true
        isLenient = true
        explicitNulls = false
        coerceInputValues = true
    }

    private val client = OkHttpClient.Builder()
        .connectTimeout(6, TimeUnit.SECONDS)
        .writeTimeout(12, TimeUnit.SECONDS)
        // The socket is long-lived; the server sends its own 25 s pings.
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .pingInterval(25, TimeUnit.SECONDS)
        .build()

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var connectJob: Job? = null
    private var pollJob: Job? = null

    @Volatile private var generation = 0
    @Volatile private var baseUrl = ""
    @Volatile private var token: String? = null
    @Volatile private var socket: WebSocket? = null

    private val _events = MutableSharedFlow<ServerEvent>(
        extraBufferCapacity = 64,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    val events = _events.asSharedFlow()

    private val _connection = MutableStateFlow<Connection>(Connection.Offline())
    val connection = _connection.asStateFlow()

    private val pending = ConcurrentHashMap<String, CompletableDeferred<ActionResult>>()

    val currentBaseUrl: String get() = baseUrl

    fun start(host: String, token: String?) {
        stop()
        val gen = ++generation
        baseUrl = baseUrlOf(host)
        this.token = token?.takeIf { it.isNotBlank() }
        connectJob = scope.launch { connectLoop(gen) }
        pollJob = scope.launch { pollLoop(gen) }
    }

    fun stop() {
        generation++
        connectJob?.cancel()
        pollJob?.cancel()
        connectJob = null
        pollJob = null
        socket?.cancel()
        socket = null
        failPending("Bağlantı kapandı")
        _connection.value = Connection.Offline()
    }

    private suspend fun connectLoop(gen: Int) {
        var attempt = 0
        while (currentCoroutineContext().isActive && gen == generation) {
            _connection.value = Connection.Connecting
            val opened = CompletableDeferred<Unit>()
            val closed = CompletableDeferred<Unit>()
            var failure: String? = null

            val listener = object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    if (gen != generation) {
                        webSocket.close(1000, null)
                        return
                    }
                    socket = webSocket
                    opened.complete(Unit)
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    if (gen != generation) return
                    handleText(text)
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    failure = when {
                        response?.code == 401 -> "Token reddedildi"
                        t is java.net.UnknownHostException -> "Sunucu adresi bulunamadı"
                        t is java.net.SocketTimeoutException -> "Sunucu yanıt vermedi"
                        t is java.net.ConnectException -> "Sunucuya ulaşılamadı"
                        t is java.net.NoRouteToHostException -> "Ağa ulaşılamıyor"
                        else -> t.message ?: "Sunucuya ulaşılamadı"
                    }
                    if (!opened.isCompleted) opened.completeExceptionally(t)
                    closed.complete(Unit)
                    if (gen == generation) failPending("Bağlantı koptu")
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    closed.complete(Unit)
                    if (gen == generation) failPending("Bağlantı kapandı")
                }

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    webSocket.close(1000, null)
                }
            }

            val request = Request.Builder().url(wsUrl()).build()
            val ws = client.newWebSocket(request, listener)

            val online = try {
                withTimeoutOrNull(8_000) { opened.await() } != null
            } catch (_: Exception) {
                false
            }

            if (online) {
                attempt = 0
                _connection.value = Connection.Online
                closed.await()
                if (gen == generation) socket = null
            } else {
                ws.cancel()
            }

            if (gen != generation) return
            _connection.value = Connection.Offline(failure)
            delay(backoff(attempt))
            attempt++
        }
    }

    private fun backoff(attempt: Int): Long {
        val base = minOf(10_000L, 450L * (1L shl attempt.coerceAtMost(5)))
        return base + (0..300).random()
    }

    private suspend fun pollLoop(gen: Int) {
        while (currentCoroutineContext().isActive && gen == generation) {
            if (_connection.value !is Connection.Online) {
                runCatching { fetchMetrics() }
            }
            delay(2_000)
        }
    }

    private fun wsUrl(): String {
        val wsBase = when {
            baseUrl.startsWith("https://") -> "wss://" + baseUrl.removePrefix("https://")
            baseUrl.startsWith("http://") -> "ws://" + baseUrl.removePrefix("http://")
            else -> "ws://$baseUrl"
        }
        val currentToken = token
        return if (currentToken == null) {
            "$wsBase/ws"
        } else {
            "$wsBase/ws?token=${URLEncoder.encode(currentToken, "UTF-8")}"
        }
    }

    private fun handleText(text: String) {
        val event = runCatching { parseEvent(text) }.getOrNull() ?: return
        if (event is ServerEvent.ActionResult) {
            val id = event.requestId ?: return
            pending.remove(id)?.complete(event.result)
        } else {
            _events.tryEmit(event)
        }
    }

    private fun parseEvent(text: String): ServerEvent {
        val obj = json.parseToJsonElement(text).jsonObject
        return when (obj["type"]?.jsonPrimitive?.contentOrNull) {
            "hello" -> ServerEvent.Hello(
                version = obj["version"]?.jsonPrimitive?.contentOrNull.orEmpty(),
                metrics = obj["metrics"]?.decodeOrNull(Metrics.serializer()),
                config = obj["config"]?.decodeOrNull(DeckConfig.serializer()),
                state = obj["state"]?.decodeOrNull(DeckState.serializer()),
            )

            "metrics" -> ServerEvent.Sample(
                json.decodeFromJsonElement(Metrics.serializer(), obj),
            )

            "config" -> {
                val config = obj["config"]?.decodeOrNull(DeckConfig.serializer())
                val state = obj["state"]?.decodeOrNull(DeckState.serializer())
                if (config == null || state == null) ServerEvent.Unknown
                else ServerEvent.ConfigUpdate(config, state)
            }

            "deck_state", "state" -> {
                val state = (obj["state"] ?: obj).decodeOrNull(DeckState.serializer())
                if (state == null) ServerEvent.Unknown else ServerEvent.StateUpdate(state)
            }

            "action_result" -> ServerEvent.ActionResult(
                requestId = obj["request_id"]?.jsonPrimitive?.contentOrNull,
                result = ActionResult(
                    ok = obj["ok"]?.jsonPrimitive?.booleanOrNull ?: false,
                    message = obj["message"]?.jsonPrimitive?.contentOrNull.orEmpty(),
                    output = obj["output"]?.jsonPrimitive?.contentOrNull,
                ),
            )

            else -> ServerEvent.Unknown
        }
    }

    private fun <T> kotlinx.serialization.json.JsonElement.decodeOrNull(
        serializer: kotlinx.serialization.DeserializationStrategy<T>,
    ): T? {
        if (this is JsonNull) return null
        return runCatching { json.decodeFromJsonElement(serializer, this) }.getOrNull()
    }

    private suspend fun fetchMetrics() {
        request("/api/metrics").use { response ->
            if (!response.isSuccessful) return
            val text = response.body?.string() ?: return
            val obj = runCatching { json.parseToJsonElement(text).jsonObject }.getOrNull() ?: return
            if (obj["type"]?.jsonPrimitive?.contentOrNull != "metrics") return
            val metrics = runCatching {
                json.decodeFromJsonElement(Metrics.serializer(), obj)
            }.getOrNull() ?: return
            _events.tryEmit(ServerEvent.Sample(metrics))
        }
    }

    /** Send an action over the socket, or fall back to HTTP when offline. */
    suspend fun action(payload: JsonObject): ActionResult {
        val ws = socket
        if (ws == null) return postAction(payload)

        val requestId = UUID.randomUUID().toString()
        val deferred = CompletableDeferred<ActionResult>()
        pending[requestId] = deferred

        val message = buildJsonObject {
            payload.forEach { (key, value) -> put(key, value) }
            put("type", "action")
            put("request_id", requestId)
        }

        if (!ws.send(message.toString())) {
            pending.remove(requestId)
            return postAction(payload)
        }

        return try {
            withTimeoutOrNull(12_000) { deferred.await() }
                ?: ActionResult(false, "Sunucudan yanıt yok")
        } finally {
            pending.remove(requestId)
        }
    }

    private suspend fun postAction(payload: JsonObject): ActionResult {
        return runCatching {
            request("/api/action", "POST", payload.toString().toRequestBody(JSON)).use { response ->
                val text = response.body?.string().orEmpty()
                if (response.code == 401) {
                    return ActionResult(false, "Token reddedildi")
                }
                val obj = runCatching { json.parseToJsonElement(text).jsonObject }.getOrNull()
                ActionResult(
                    ok = obj?.get("ok")?.jsonPrimitive?.booleanOrNull ?: false,
                    message = obj?.get("message")?.jsonPrimitive?.contentOrNull
                        ?: "Sunucu yanıt vermedi",
                    output = obj?.get("output")?.jsonPrimitive?.contentOrNull,
                )
            }
        }.getOrElse { ActionResult(false, it.message ?: "Sunucuya ulaşılamadı") }
    }

    suspend fun postState(board: String, button: String, active: Boolean) {
        val payload = buildJsonObject {
            put("board", board)
            put("button", button)
            put("active", active)
        }
        runCatching {
            request("/api/state", "POST", payload.toString().toRequestBody(JSON)).close()
        }
    }

    suspend fun postActiveBoard(board: String) {
        val payload = buildJsonObject { put("active_board", board) }
        runCatching {
            request("/api/state", "POST", payload.toString().toRequestBody(JSON)).close()
        }
    }

    /** Replace the whole deck config; the server normalises ids, colours and limits. */
    suspend fun putConfig(config: DeckConfig): ConfigSave {
        val body = runCatching {
            json.encodeToString(DeckConfig.serializer(), config)
        }.getOrElse { return ConfigSave(false, it.message ?: "Pano kaydedilemedi", null) }

        return runCatching {
            request("/api/config", "PUT", body.toRequestBody(JSON)).use { response ->
                val text = response.body?.string().orEmpty()
                val obj = runCatching { json.parseToJsonElement(text).jsonObject }.getOrNull()
                if (response.code == 401) {
                    return ConfigSave(false, "Token reddedildi", null)
                }
                val parsed = ConfigSave(
                    ok = obj?.get("ok")?.jsonPrimitive?.booleanOrNull ?: false,
                    message = obj?.get("message")?.jsonPrimitive?.contentOrNull.orEmpty(),
                    config = obj?.get("config")?.decodeOrNull(DeckConfig.serializer()),
                )
                if (!parsed.ok) {
                    Log.w(TAG, "config PUT ${response.code} rejected: ${parsed.message}")
                }
                parsed
            }
        }.getOrElse {
            Log.w(TAG, "config PUT failed", it)
            ConfigSave(false, it.message ?: "Sunucuya ulaşılamadı", null)
        }
    }

    /**
     * Runs a request and buffers the body while still on the IO dispatcher.
     * Callers are often on the main thread, and touching the socket from there
     * throws `NetworkOnMainThreadException` on Android.
     */
    private suspend fun request(
        path: String,
        method: String = "GET",
        body: RequestBody? = null,
    ): Response = withContext(Dispatchers.IO) {
        val builder = Request.Builder().url(baseUrl + path)
        token?.let { builder.header("X-Liquid-Token", it) }
        when (method) {
            "POST" -> builder.post(body ?: ByteArray(0).toRequestBody(null))
            "PUT" -> builder.put(body ?: ByteArray(0).toRequestBody(null))
            else -> builder.get()
        }
        client.newCall(builder.build()).execute().use { response ->
            val source = response.body
            val bytes = source?.bytes() ?: ByteArray(0)
            response.newBuilder()
                .body(bytes.toResponseBody(source?.contentType()))
                .build()
        }
    }

    private fun failPending(reason: String) {
        val snapshot = pending.values.toList()
        pending.clear()
        snapshot.forEach { it.complete(ActionResult(false, reason)) }
    }

    fun shutdown() {
        stop()
        scope.cancel()
    }

    companion object {
        private val probeClient: OkHttpClient by lazy {
            OkHttpClient.Builder()
                .connectTimeout(5, TimeUnit.SECONDS)
                .readTimeout(5, TimeUnit.SECONDS)
                .build()
        }

        private val probeJson = Json { ignoreUnknownKeys = true }

        fun baseUrlOf(host: String): String {
            val trimmed = host.trim().trimEnd('/')
            return when {
                trimmed.startsWith("http://") || trimmed.startsWith("https://") -> trimmed
                else -> "http://$trimmed"
            }
        }

        /** Verify a host/token pair before the shell opens. */
        suspend fun probe(host: String, token: String?): ProbeResult = withContext(Dispatchers.IO) {
            val base = baseUrlOf(host)
            val client = probeClient

            val healthRequest = Request.Builder().url("$base/api/health").build()
            val health = runCatching {
                client.newCall(healthRequest).execute().use { response ->
                    if (!response.isSuccessful) return@use null
                    val text = response.body?.string() ?: return@use null
                    probeJson.parseToJsonElement(text).jsonObject
                }
            }.getOrNull() ?: return@withContext ProbeResult(
                false,
                "Sunucuya ulaşılamadı. PC'de liquid-deck çalışıyor mu?",
            )

            val version = health["version"]?.jsonPrimitive?.contentOrNull
            val hostName = health["host"]?.jsonPrimitive?.contentOrNull
            val authRequired = health["auth_required"]?.jsonPrimitive?.booleanOrNull ?: false

            if (authRequired && token.isNullOrBlank()) {
                return@withContext ProbeResult(
                    false,
                    "Bu sunucu token istiyor.",
                    version,
                    hostName,
                    true,
                )
            }

            if (!token.isNullOrBlank()) {
                val authRequest = Request.Builder()
                    .url("$base/api/auth")
                    .header("X-Liquid-Token", token)
                    .build()
                val authorized = runCatching {
                    client.newCall(authRequest).execute().use { it.isSuccessful }
                }.getOrDefault(false)
                if (!authorized) {
                    return@withContext ProbeResult(
                        false,
                        "Token reddedildi.",
                        version,
                        hostName,
                        authRequired,
                    )
                }
            }

            ProbeResult(true, "Bağlantı hazır", version, hostName, authRequired)
        }
    }
}
