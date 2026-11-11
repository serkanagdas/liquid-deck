package com.liquiddeck.app.data

import kotlinx.serialization.Serializable

// ---------------------------------------------------------------------------
// AI subscription quota tracking.  Providers are configured on the phone and
// queried directly, so the PC does not have to be online for this page.
// ---------------------------------------------------------------------------

@Serializable
data class QuotaProvider(
    val id: String = "",
    val kind: String = "zai",
    val label: String = "",
    val key: String = "",
    val budget: Double? = null,
    val manualRemaining: Int = 100,
    val note: String = "",
)

@Serializable
data class QuotaWindow(
    val label: String,
    /** 100 = full quota left, 0 = exhausted.  Null when the provider only reports a balance. */
    val remaining: Float? = null,
    val used: Float? = null,
    val detail: String? = null,
    /** Epoch millis of the next reset, when the provider reports one. */
    val resetAt: Long? = null,
)

data class QuotaSnapshot(
    val providerId: String,
    val windows: List<QuotaWindow> = emptyList(),
    val plan: String? = null,
    /** Headline text for providers that report money instead of a percentage. */
    val balance: String? = null,
    val error: String? = null,
    val updatedAt: Long = 0L,
) {
    val primary: QuotaWindow? get() = windows.firstOrNull()
}

data class QuotaState(
    val providers: List<QuotaProvider> = emptyList(),
    val snapshots: Map<String, QuotaSnapshot> = emptyMap(),
    val refreshing: Set<String> = emptySet(),
    val loaded: Boolean = false,
)

/**
 * Supported sources.  Only these have a public usage endpoint; everything
 * else (ChatGPT Plus, Gemini, Copilot, Cursor, …) is tracked through [MANUAL]
 * because the vendors do not expose a subscription-quota API.
 */
enum class QuotaKind(
    val id: String,
    val label: String,
    val keyHint: String,
    val needsKey: Boolean = true,
    val supportsBudget: Boolean = false,
) {
    ZAI(
        id = "zai",
        label = "Z.ai",
        keyHint = "z.ai → API Keys → kişisel Coding Plan anahtarı (5 saatlik ve haftalık kota).",
    ),
    CLAUDE(
        id = "claude",
        label = "Claude",
        keyHint = "Claude Code OAuth token'ı: ~/.claude/.credentials.json → accessToken.",
    ),
    OPENROUTER(
        id = "openrouter",
        label = "OpenRouter",
        keyHint = "OpenRouter anahtarı (sk-or-…). Kredi ve anahtar limiti okunur.",
        supportsBudget = true,
    ),
    DEEPSEEK(
        id = "deepseek",
        label = "DeepSeek",
        keyHint = "DeepSeek API anahtarı (sk-…). Kalan bakiye okunur.",
        supportsBudget = true,
    ),
    OPENAI(
        id = "openai",
        label = "OpenAI",
        keyHint = "OpenAI admin anahtarı (org:read). ChatGPT aboneliği için API yok, harcama okunur.",
        supportsBudget = true,
    ),
    MANUAL(
        id = "manual",
        label = "Manuel",
        keyHint = "Anahtar gerekmez; kalan yüzdeyi kendin güncellersin.",
        needsKey = false,
    ),
    ;

    companion object {
        fun from(id: String?): QuotaKind = entries.firstOrNull { it.id == id } ?: MANUAL
    }
}
