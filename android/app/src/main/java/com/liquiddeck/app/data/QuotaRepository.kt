package com.liquiddeck.app.data

import java.net.ConnectException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import java.util.Calendar
import java.util.Locale
import java.util.TimeZone
import java.util.concurrent.TimeUnit
import javax.net.ssl.SSLException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONException
import org.json.JSONObject

/**
 * Talks to the subscription APIs directly from the phone.  Every adapter is
 * defensive: vendors change field names, so a missing field degrades to
 * "no data" instead of crashing, and all errors are turned into one readable
 * Turkish line.
 */
class QuotaRepository {
    private val client = OkHttpClient.Builder()
        .connectTimeout(15, TimeUnit.SECONDS)
        .readTimeout(20, TimeUnit.SECONDS)
        .callTimeout(30, TimeUnit.SECONDS)
        .build()

    suspend fun fetch(provider: QuotaProvider): QuotaSnapshot = withContext(Dispatchers.IO) {
        val now = System.currentTimeMillis()
        try {
            val snapshot = when (QuotaKind.from(provider.kind)) {
                QuotaKind.ZAI -> zai(provider)
                QuotaKind.CLAUDE -> claude(provider)
                QuotaKind.OPENROUTER -> openRouter(provider)
                QuotaKind.DEEPSEEK -> deepSeek(provider)
                QuotaKind.OPENAI -> openAi(provider)
                QuotaKind.MANUAL -> manual(provider)
            }
            snapshot.copy(providerId = provider.id, updatedAt = now)
        } catch (error: Exception) {
            QuotaSnapshot(
                providerId = provider.id,
                error = describe(error),
                updatedAt = now,
            )
        }
    }

    // -----------------------------------------------------------------------
    // Z.ai coding plan
    // -----------------------------------------------------------------------

    private fun zai(provider: QuotaProvider): QuotaSnapshot {
        val root = get("https://api.z.ai/api/monitor/usage/quota/limit", auth(provider))
        val code = root.optInt("code", 200)
        if (code in 400..599) {
            throw HttpError(code, root.optString("msg"))
        }
        val data = root.optJSONObject("data") ?: root
        val limits = data.optJSONArray("limits") ?: root.optJSONArray("limits")
        val windows = mutableListOf<QuotaWindow>()
        if (limits != null) {
            for (index in 0 until limits.length()) {
                val row = limits.optJSONObject(index) ?: continue
                val used = row.optDouble("percentage", Double.NaN)
                if (used.isNaN()) continue
                windows += QuotaWindow(
                    label = zaiWindowLabel(row),
                    remaining = (100.0 - used).toFloat().coerceIn(0f, 100f),
                    used = used.toFloat().coerceIn(0f, 100f),
                    resetAt = row.optLong("nextResetTime", 0L).takeIf { it > 0L },
                )
            }
        }
        if (windows.isEmpty()) {
            throw IllegalStateException("Kota penceresi bulunamadı. Coding Plan anahtarı mı?")
        }
        val plan = data.firstString("planName", "plan", "level", "planType")
            ?: root.firstString("planName", "plan")
        return QuotaSnapshot(
            providerId = provider.id,
            // Shortest window first (5-hour before the long period), which is
            // the order you actually watch a coding plan in.
            windows = windows.sortedBy { it.resetAt ?: Long.MAX_VALUE },
            plan = plan,
        )
    }

    private fun zaiWindowLabel(row: JSONObject): String {
        val unit = row.optInt("unit", 0)
        val number = row.optInt("number", 0)
        return when (row.optString("type")) {
            "TOKENS_LIMIT" -> when (unit) {
                6 -> "7 gün"
                3 -> if (number > 0) "$number saat" else "5 saat"
                5 -> if (number > 0) "$number dk" else "Dakika"
                else -> "Token"
            }
            "CREDIT_LIMIT" -> "Kredi"
            // The plan reports a unitless budget that resets with the billing
            // period, so it is shown as a period rather than as hours.
            "TIME_LIMIT" -> "Dönem"
            else -> row.optString("type").ifBlank { "Kota" }
        }
    }

    // -----------------------------------------------------------------------
    // Claude (Claude Code OAuth token; undocumented but stable)
    // -----------------------------------------------------------------------

    private fun claude(provider: QuotaProvider): QuotaSnapshot {
        val root = get(
            url = "https://api.anthropic.com/api/oauth/usage",
            headers = auth(provider) + mapOf(
                "anthropic-beta" to "oauth-2025-04-20",
                "anthropic-version" to "2023-06-01",
            ),
        )
        val windows = buildList {
            claudeWindow(root.optJSONObject("five_hour"), "5 saat")?.let(::add)
            claudeWindow(root.optJSONObject("seven_day"), "7 gün")?.let(::add)
        }
        if (windows.isEmpty()) {
            throw IllegalStateException("Kullanım penceresi bulunamadı.")
        }
        val extra = root.optJSONObject("extra_usage")
        val plan = if (extra?.optBoolean("is_enabled") == true) "Ek kullanım açık" else null
        return QuotaSnapshot(
            providerId = provider.id,
            windows = windows,
            plan = plan,
        )
    }

    private fun claudeWindow(window: JSONObject?, label: String): QuotaWindow? {
        if (window == null) return null
        val used = window.optDouble("utilization", Double.NaN)
        if (used.isNaN()) return null
        return QuotaWindow(
            label = label,
            remaining = (100.0 - used).toFloat().coerceIn(0f, 100f),
            used = used.toFloat().coerceIn(0f, 100f),
            resetAt = parseIsoMillis(window.optString("resets_at")),
        )
    }

    // -----------------------------------------------------------------------
    // OpenRouter credits / key limit
    // -----------------------------------------------------------------------

    private fun openRouter(provider: QuotaProvider): QuotaSnapshot {
        val headers = auth(provider)
        val root = get("https://openrouter.ai/api/v1/credits", headers)
        val data = root.optJSONObject("data") ?: root
        val total = data.optDouble("total_credits", 0.0)
        val spent = data.optDouble("total_usage", 0.0)
        val creditsLeft = (total - spent).coerceAtLeast(0.0)

        val key = runCatching {
            val keyRoot = get("https://openrouter.ai/api/v1/key", headers)
            keyRoot.optJSONObject("data") ?: keyRoot
        }.getOrNull()
        val keyLimit = key?.optDouble("limit", Double.NaN)?.takeIf { !it.isNaN() && it > 0 }
        val keyRemaining = key?.optDouble("limit_remaining", Double.NaN)?.takeIf { !it.isNaN() }
        val budget = provider.budget?.takeIf { it > 0 }

        val windows = mutableListOf<QuotaWindow>()
        if (keyLimit != null && keyRemaining != null) {
            windows += QuotaWindow(
                label = "Anahtar limiti",
                remaining = (keyRemaining / keyLimit * 100).toFloat().coerceIn(0f, 100f),
                used = ((keyLimit - keyRemaining) / keyLimit * 100).toFloat().coerceIn(0f, 100f),
                detail = "${usd(keyRemaining)} / ${usd(keyLimit)}",
            )
        } else if (budget != null) {
            windows += QuotaWindow(
                label = "Kredi",
                remaining = (creditsLeft / budget * 100).toFloat().coerceIn(0f, 100f),
                used = (spent / budget * 100).toFloat().coerceIn(0f, 100f),
                detail = "${usd(creditsLeft)} / ${usd(budget)}",
            )
        }

        return QuotaSnapshot(
            providerId = provider.id,
            windows = windows,
            plan = key?.firstString("label")?.takeIf { !it.equals("default", ignoreCase = true) },
            balance = if (windows.isEmpty()) "${usd(creditsLeft)} kaldı" else null,
        )
    }

    // -----------------------------------------------------------------------
    // DeepSeek balance
    // -----------------------------------------------------------------------

    private fun deepSeek(provider: QuotaProvider): QuotaSnapshot {
        val root = get("https://api.deepseek.com/user/balance", auth(provider))
        val info = root.optJSONArray("balance_infos")?.optJSONObject(0)
        if (info == null) {
            return QuotaSnapshot(
                providerId = provider.id,
                balance = if (root.optBoolean("is_available", false)) "Bakiye var" else "Bakiye yok",
            )
        }
        val total = info.optString("total_balance")
        val currency = info.optString("currency", "USD")
        val money = money(total, currency)
        val budget = provider.budget?.takeIf { it > 0 }
        val windows = mutableListOf<QuotaWindow>()
        val left = total.replace(",", "").toDoubleOrNull()
        if (budget != null && left != null) {
            windows += QuotaWindow(
                label = "Bakiye",
                remaining = (left / budget * 100).toFloat().coerceIn(0f, 100f),
                used = ((budget - left) / budget * 100).toFloat().coerceIn(0f, 100f),
                detail = "$money / ${money(budget.toString(), currency)}",
            )
        }
        return QuotaSnapshot(
            providerId = provider.id,
            windows = windows,
            balance = if (windows.isEmpty()) money else null,
            plan = if (root.optBoolean("is_available", true)) null else "Bakiye yok",
        )
    }

    // -----------------------------------------------------------------------
    // OpenAI organisation costs (admin key; spend, not subscription quota)
    // -----------------------------------------------------------------------

    private fun openAi(provider: QuotaProvider): QuotaSnapshot {
        val start = startOfMonthSeconds()
        val root = get(
            url = "https://api.openai.com/v1/organization/costs" +
                "?start_time=$start&bucket_width=1d&limit=31",
            headers = auth(provider),
        )
        val buckets = root.optJSONArray("data")
        var spent = 0.0
        var currency = "usd"
        if (buckets != null) {
            for (index in 0 until buckets.length()) {
                val results = buckets.optJSONObject(index)?.optJSONArray("results") ?: continue
                for (row in 0 until results.length()) {
                    val amount = results.optJSONObject(row)?.optJSONObject("amount") ?: continue
                    spent += amount.optDouble("value", 0.0)
                    amount.optString("currency").takeIf { it.isNotBlank() }?.let { currency = it }
                }
            }
        }
        val budget = provider.budget?.takeIf { it > 0 }
        val windows = mutableListOf<QuotaWindow>()
        if (budget != null) {
            windows += QuotaWindow(
                label = "Aylık bütçe",
                remaining = ((budget - spent) / budget * 100).toFloat().coerceIn(0f, 100f),
                used = (spent / budget * 100).toFloat().coerceIn(0f, 100f),
                detail = "${usd(spent)} / ${usd(budget)}",
            )
        }
        return QuotaSnapshot(
            providerId = provider.id,
            windows = windows,
            balance = if (windows.isEmpty()) "${usd(spent)} bu ay" else null,
            plan = if (currency.equals("usd", ignoreCase = true)) null else currency.uppercase(Locale.US),
        )
    }

    // -----------------------------------------------------------------------
    // Manual entry
    // -----------------------------------------------------------------------

    private fun manual(provider: QuotaProvider): QuotaSnapshot {
        val remaining = provider.manualRemaining.coerceIn(0, 100)
        return QuotaSnapshot(
            providerId = provider.id,
            windows = listOf(
                QuotaWindow(
                    label = "Kalan",
                    remaining = remaining.toFloat(),
                    used = (100 - remaining).toFloat(),
                ),
            ),
        )
    }

    // -----------------------------------------------------------------------
    // Plumbing
    // -----------------------------------------------------------------------

    private fun get(url: String, headers: Map<String, String> = emptyMap()): JSONObject {
        val builder = Request.Builder()
            .url(url)
            .get()
            .header("Accept", "application/json")
            .header("User-Agent", "LiquidDeck-Android/1.0")
        headers.forEach { (name, value) -> builder.header(name, value) }
        client.newCall(builder.build()).execute().use { response ->
            val body = response.body?.string().orEmpty()
            if (!response.isSuccessful) {
                throw HttpError(response.code, body.take(240))
            }
            if (body.isBlank()) throw IllegalStateException("Boş yanıt.")
            return JSONObject(body)
        }
    }

    private fun auth(provider: QuotaProvider): Map<String, String> =
        mapOf("Authorization" to "Bearer ${provider.key.trim()}")

    private fun describe(error: Throwable): String = when (error) {
        is HttpError -> when (error.code) {
            401, 403 -> "Anahtar reddedildi (${error.code}). Anahtarı kontrol et."
            404 -> "Uç nokta bulunamadı (404)."
            429 -> "Çok fazla istek (429). Birkaç dakika sonra dene."
            in 500..599 -> "Sağlayıcı sunucusu hata verdi (${error.code})."
            else -> "Sunucu ${error.code} döndü."
        }
        is UnknownHostException, is ConnectException, is SocketTimeoutException, is SSLException ->
            "Ağa ulaşılamadı. İnternet bağlantısını kontrol et."
        is JSONException -> "Yanıt anlaşılamadı."
        else -> error.message?.take(140) ?: "Bilinmeyen hata."
    }

    private fun usd(value: Double): String =
        "$" + String.format(Locale.US, "%.2f", value)

    private fun money(raw: String, currency: String): String {
        val symbol = when (currency.uppercase(Locale.US)) {
            "USD" -> "$"
            "CNY", "RMB" -> "¥"
            "EUR" -> "€"
            else -> "${currency.uppercase(Locale.US)} "
        }
        return symbol + raw
    }

    private fun startOfMonthSeconds(): Long {
        val calendar = Calendar.getInstance()
        calendar.set(Calendar.DAY_OF_MONTH, 1)
        calendar.set(Calendar.HOUR_OF_DAY, 0)
        calendar.set(Calendar.MINUTE, 0)
        calendar.set(Calendar.SECOND, 0)
        calendar.set(Calendar.MILLISECOND, 0)
        return calendar.timeInMillis / 1000
    }

    private class HttpError(val code: Int, body: String) : Exception("HTTP $code ${body.take(120)}")
}

private val ISO_DATE = Regex(
    """(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,9}))?(Z|[+-]\d{2}:?\d{2})""",
)

/** ISO-8601 → epoch millis without java.time (minSdk 23). */
private fun parseIsoMillis(text: String?): Long? {
    if (text.isNullOrBlank()) return null
    val match = ISO_DATE.find(text) ?: return null
    val parts = match.groupValues
    val calendar = Calendar.getInstance(TimeZone.getTimeZone("UTC"))
    calendar.clear()
    calendar.set(
        parts[1].toInt(),
        parts[2].toInt() - 1,
        parts[3].toInt(),
        parts[4].toInt(),
        parts[5].toInt(),
        parts[6].toInt(),
    )
    var millis = calendar.timeInMillis
    parts[7].takeIf { it.isNotBlank() }?.let { fraction ->
        millis += fraction.padEnd(3, '0').take(3).toLong()
    }
    val zone = parts[8]
    if (zone != "Z") {
        val sign = if (zone.startsWith("-")) -1 else 1
        val digits = zone.drop(1).replace(":", "")
        val hours = digits.take(2).toIntOrNull() ?: 0
        val minutes = digits.drop(2).take(2).toIntOrNull() ?: 0
        millis -= sign * (hours * 3_600_000L + minutes * 60_000L)
    }
    return millis
}

private fun JSONObject.firstString(vararg names: String): String? =
    names.mapNotNull { name -> opt(name) as? String }
        .firstOrNull { it.isNotBlank() }
