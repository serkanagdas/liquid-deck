/* Lightweight navigation, phone-local preferences/history and TCP jitter alerts. */
(() => {
  "use strict";
  const $ = (id) => document.getElementById(id);
  const KEY = "liquid-deck.panel.v1";
  const defaults = { finish: "glass", scale: "large", logging: true, threshold: 30 };
  let preferences = Object.assign({}, defaults);
  try { preferences = Object.assign(preferences, JSON.parse(localStorage.getItem(KEY) || "{}")); } catch { /* private mode */ }
  if (!["glass", "flat"].includes(preferences.finish)) preferences.finish = defaults.finish;
  if (!["normal", "large"].includes(preferences.scale)) preferences.scale = defaults.scale;
  if (![15, 30, 60].includes(preferences.threshold)) preferences.threshold = defaults.threshold;
  preferences.logging = preferences.logging !== false;
  let logger = null;
  let source = "unknown";
  let rows = [];
  let historyGeneration = 0;
  let lastSampleTs = 0;
  let previousPing = null;
  let previousPingTs = 0;
  let lastAlert = 0;
  let lastDelta = null;
  let page = "system";
  let gesture = null;
  let suppressClickUntil = 0;
  let currentMetrics = null;
  const text = (id, value) => { const node = $(id); if (node && node.textContent !== value) node.textContent = value; };

  function ensureLogger() {
    if (!logger && preferences.logging) logger = new LiquidHistory.MinuteLog((status) => {
      text("localLogStatus", preferences.logging ? status : "Yerel kayıt kapalı");
    });
    return logger;
  }
  function savePreferences() {
    document.body.dataset.finish = preferences.finish;
    document.body.dataset.scale = preferences.scale;
    try { localStorage.setItem(KEY, JSON.stringify(preferences)); } catch { /* session-only settings */ }
    if (preferences.logging) ensureLogger();
    else text("localLogStatus", "Yerel kayıt kapalı");
  }
  savePreferences();
  $("finishSelect").value = preferences.finish;
  $("scaleSelect").value = preferences.scale;
  $("loggingEnabled").checked = preferences.logging;
  $("jitterThreshold").value = String(preferences.threshold);
  for (const [id, field] of [["finishSelect", "finish"], ["scaleSelect", "scale"]]) {
    $(id).addEventListener("change", (event) => { preferences[field] = event.target.value; savePreferences(); });
  }
  $("loggingEnabled").addEventListener("change", (event) => {
    if (logger) logger.flush();
    preferences.logging = event.target.checked;
    savePreferences();
  });
  $("jitterThreshold").addEventListener("change", (event) => { preferences.threshold = Number(event.target.value); savePreferences(); });

  // Native dialogs trap focus; fallback supports older Note 4 WebViews.
  let dialogFocus = null;
  function openDialog(id) {
    const dialog = $(id);
    dialogFocus = document.activeElement;
    if (typeof dialog.showModal === "function") dialog.showModal();
    else { dialog.setAttribute("open", ""); dialog.classList.add("dialog-fallback"); }
    dialog.querySelector("button").focus();
  }
  function closeDialog(id) {
    const dialog = $(id);
    if (typeof dialog.close === "function") dialog.close();
    else dialog.removeAttribute("open");
    if (dialogFocus) dialogFocus.focus();
  }
  $("preferencesOpen").addEventListener("click", () => openDialog("preferencesDialog"));
  document.querySelectorAll("[data-dismiss]").forEach((button) => button.addEventListener("click", () => closeDialog(button.dataset.dismiss)));
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    document.querySelectorAll(".dialog-fallback[open]").forEach((dialog) => closeDialog(dialog.id));
  });

  function navigate(next) {
    page = next;
    document.body.dataset.page = page;
    $("systemPage").hidden = page !== "system";
    $("deckPage").hidden = page !== "deck";
    document.querySelectorAll("[data-page]").forEach((button) => button.setAttribute("aria-pressed", String(button.dataset.page === page)));
    document.dispatchEvent(new CustomEvent("liquid:page"));
  }
  navigate(page);
  document.querySelectorAll("[data-page]").forEach((button) => button.addEventListener("click", () => navigate(button.dataset.page)));
  const pages = $("dashboardPages");
  pages.addEventListener("pointerdown", (event) => {
    if (!event.isPrimary || (event.pointerType === "mouse" && event.button !== 0) || event.target.closest(".board-bar, input, select")) return;
    gesture = { x: event.clientX, y: event.clientY, id: event.pointerId, moved: false };
  });
  pages.addEventListener("pointermove", (event) => {
    if (!gesture || gesture.id !== event.pointerId) return;
    if (Math.abs(event.clientX - gesture.x) > 12) {
      gesture.moved = true;
      suppressClickUntil = Date.now() + 500;
    }
  }, { passive: true });
  pages.addEventListener("pointerup", (event) => {
    if (!gesture || gesture.id !== event.pointerId) return;
    const dx = event.clientX - gesture.x;
    const dy = event.clientY - gesture.y;
    const swipe = Math.abs(dx) > 55 && Math.abs(dx) > Math.abs(dy) * 1.5;
    if (swipe) {
      event.preventDefault();
      suppressClickUntil = Date.now() + 500;
      navigate(dx < 0 ? "deck" : "system");
    }
    gesture = null;
  });
  pages.addEventListener("pointercancel", () => { gesture = null; });
  // Some Android WebViews synthesize a delayed click after pointerup, even
  // after the page's contents changed. Block it before any action handlers.
  document.addEventListener("click", (event) => {
    if (Date.now() < suppressClickUntil && pages.contains(event.target)) {
      suppressClickUntil = 0;
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  }, true);
  // The key handlers also cancel long-press and action execution after movement.

  function ingest(metrics, live) {
    if (!metrics || typeof metrics.ts !== "number") return;
    const nextSource = String(metrics.host || "unknown");
    if (nextSource !== source) {
      source = nextSource;
      previousPing = null;
      previousPingTs = 0;
      lastSampleTs = 0;
      lastDelta = null;
      lastAlert = 0;
    }
    currentMetrics = metrics;
    const freshSample = metrics.ts > lastSampleTs;
    if (freshSample) lastSampleTs = metrics.ts;
    const freshPing = typeof metrics.ping_ts === "number" && metrics.ping_ts > previousPingTs;
    let alert = false;
    if (freshPing) {
      const valid = typeof metrics.ping === "number" && Number.isFinite(metrics.ping);
      lastDelta = LiquidHistory.pingDelta(previousPing, previousPingTs, metrics.ping, metrics.ping_ts);
      previousPing = valid ? metrics.ping : null;
      previousPingTs = metrics.ping_ts;
      alert = live && lastDelta !== null && lastDelta >= preferences.threshold && Date.now() - lastAlert >= 60000;
      if (alert) lastAlert = Date.now();
    }
    if (freshSample && metrics.ping_ts && metrics.ts - metrics.ping_ts > 15000) {
      previousPing = null;
      lastDelta = null;
    }
    if (live && freshSample && preferences.logging) ensureLogger().ingest(metrics, lastDelta, alert, freshPing);
    if (!document.hidden) paint();
  }
  function paint() {
    const temp = currentMetrics && currentMetrics.temp;
    const chip = $("cpuTemp");
    if (chip) {
      chip.dataset.heat = typeof temp !== "number" ? "unknown" : temp >= 85 ? "hot" : temp >= 70 ? "warm" : "cool";
      chip.title = currentMetrics && currentMetrics.temp_source ? currentMetrics.temp_source : "CPU sıcaklık sensörü";
      if (currentMetrics && temp === null) chip.title = "Sıcaklık sensörü okunamıyor";
    }
    const brand = $("cpuBrand");
    if (brand) brand.hidden = !(currentMetrics && currentMetrics.temp_source && currentMetrics.temp_source.startsWith("AMD"));
    // Gecikme sıçraması alt bardaki şeridin çerçevesini renklendirir.
    const strip = document.querySelector(".network-strip");
    if (strip) strip.dataset.alert = String(lastAlert > 0 && Date.now() - lastAlert < 10000);
    tick();
  }
  function tick() {
    if (document.hidden) return;
    const strip = document.querySelector(".network-strip");
    if (strip && lastAlert && Date.now() - lastAlert >= 10000 && strip.dataset.alert === "true") {
      strip.dataset.alert = "false";
    }
  }

  async function loadHistory() {
    const generation = ++historyGeneration;
    const host = source;
    text("historySummary", "Kayıtlar yükleniyor…");
    $("historyExport").disabled = true;
    try {
      // Reading old history is available even after recording is turned off.
      if (!logger) logger = new LiquidHistory.MinuteLog((status) => text("localLogStatus", preferences.logging ? status : "Yerel kayıt kapalı"));
      const result = await logger.read(host);
      if (generation !== historyGeneration) return;
      rows = result;
      const samples = rows.reduce((sum, row) => sum + row.samples, 0);
      text("historySummary", rows.length ? `${host} · ${rows.length} dakikalık kayıt · ${samples} telemetri örneği` : "Henüz kayıt yok. Panel açıkken dakika özetleri bu cihazda birikir.");
      $("historyExport").disabled = !rows.length;
      renderHistory();
    } catch (error) {
      if (generation !== historyGeneration) return;
      rows = [];
      text("historySummary", error.message || "Geçmiş okunamadı");
      renderHistory();
    }
  }
  $("historyOpen").addEventListener("click", () => { openDialog("historyDialog"); loadHistory(); });
  $("historyMetric").addEventListener("change", renderHistory);

  function renderHistory() {
    const svg = $("historyChart");
    while (svg.firstChild) svg.removeChild(svg.firstChild);
    const field = $("historyMetric").value;
    const divisor = field === "net_rx" || field === "net_tx" ? 1024 : 1;
    const end = Date.now();
    const start = end - LiquidHistory.DAY;
    // At most 288 five-minute points, drawn only on explicit history interaction.
    const groups = new Map();
    for (const row of rows) {
      const stat = row.values[field];
      if (!stat || !stat.n) continue;
      const t = Math.floor(row.minute / 300000) * 300000;
      const point = groups.get(t) || { t, n: 0, sum: 0, min: stat.min, max: stat.max, minutes: [] };
      point.n += stat.n; point.sum += stat.sum;
      point.min = Math.min(point.min, stat.min); point.max = Math.max(point.max, stat.max);
      point.minutes.push(row.minute);
      groups.set(t, point);
    }
    const points = Array.from(groups.values());
    const max = Math.max(["cpu", "ram_pct", "gpu_usage", "gpu_memory_pct"].includes(field) ? 100 : 1, ...points.map((p) => p.max / divisor));
    const x = (t) => 8 + Math.max(0, Math.min(1, (t - start) / LiquidHistory.DAY)) * 584;
    const y = (value) => 148 - (value / divisor / max) * 128;
    function node(name, attributes) {
      const element = document.createElementNS("http://www.w3.org/2000/svg", name);
      Object.keys(attributes).forEach((key) => element.setAttribute(key, attributes[key]));
      svg.appendChild(element); return element;
    }
    node("path", { d: "M8 148H592", stroke: "#303843", fill: "none" });
    let line = "";
    let range = "";
    let previous = null;
    for (const point of points) {
      const px = x(point.t + 150000).toFixed(1);
      const gap = !previous || Math.min(...point.minutes) - Math.max(...previous.minutes) > 90000;
      line += `${gap ? "M" : "L"}${px} ${y(point.sum / point.n).toFixed(1)}`;
      range += `M${px} ${y(point.min).toFixed(1)}V${y(point.max).toFixed(1)}`;
      previous = point;
    }
    node("path", { d: range, stroke: "#3b6b7a", "stroke-width": "2", fill: "none" });
    node("path", { d: line, stroke: "#64d2ff", "stroke-width": "2", fill: "none" });
    // Show isolated one-bucket records too (a move-only path has no stroke).
    for (const point of points) node("circle", { cx: x(point.t + 150000), cy: y(point.sum / point.n), r: "1.7", fill: "#64d2ff" });
    const label = node("text", { x: "12", y: "16", fill: "#b7c6d3", "font-size": "10" });
    label.textContent = `0–${max.toFixed(1)} · ${points.length ? "ortalama + min–maks" : "bu sensör için veri yok"}`;
    text("historyStart", new Date(start).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) + " · dün");
    const list = $("historyEvents");
    list.textContent = "";
    const events = rows.flatMap((row) => row.events).slice(-30).reverse();
    for (const event of events) {
      const li = document.createElement("li");
      li.textContent = `${new Date(event.t).toLocaleTimeString()} · Fark ${event.delta.toFixed(1)} ms · Ping ${event.ping.toFixed(1)} ms`;
      list.appendChild(li);
    }
    if (!events.length) { const li = document.createElement("li"); li.textContent = "Kaydedilmiş gecikme uyarısı yok."; list.appendChild(li); }
  }

  $("historyExport").addEventListener("click", () => {
    const blob = new Blob([JSON.stringify({ version: 1, source, exported_at: new Date().toISOString(), units: { net_rx: "bytes/s", net_tx: "bytes/s", temp: "C", ping: "ms", jitter: "ms", gpu_usage: "%", gpu_temp: "C", gpu_memory_pct: "%", gpu_power_watts: "W", gpu_clock_mhz: "MHz" }, minutes: rows }, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url; link.download = "liquid-deck-24h.json"; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  });
  $("historyClear").addEventListener("click", async () => {
    if (!logger || !window.confirm("Bu bilgisayarın bu tarayıcıdaki kayıtları silinsin mi?")) return;
    try { await logger.clear(source); await loadHistory(); }
    catch (error) { text("historySummary", error.message || "Geçmiş silinemedi"); }
  });
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) { if (logger) logger.flush(); }
    else paint();
  });
  window.addEventListener("pagehide", () => { if (logger) logger.flush(); });
  window.LiquidDashboard = { ingest, tick, setPingTarget(target) { document.querySelector(".network-strip").title = `TCP hedefi: ${target || "İnternet"}`; } };
})();
