/* Liquid Deck — vanilla JS client
   WebSocket telemetry, SVG gauges, touch macro deck. No dependencies. */

(() => {
  "use strict";

  const HISTORY_LEN = 60;
  const RING_CIRCUMFERENCE = 2 * Math.PI * 54;
  const TOKEN_KEY = "liquid-deck.token";
  const MAX_RECONNECT_MS = 10_000;
  const ACTION_TIMEOUT_MS = 12_000;

  const $ = (selector, root = document) => root.querySelector(selector);
  const $$ = (selector, root = document) => Array.from(root.querySelectorAll(selector));

  const els = {
    clockTime: $("#clockTime"),
    clockSeconds: $("#clockSeconds"),
    clockDate: $("#clockDate"),
    brandHost: $("#brandHost"),
    pingPill: $("#pingPill"),
    pingValue: $("#pingValue"),
    connPill: $("#connPill"),
    connLabel: $("#connLabel"),
    reconnectBanner: $("#reconnectBanner"),
    reconnectText: $("#reconnectText"),

    cpuRing: $("#cpuRing"),
    cpuValue: $("#cpuValue"),
    cpuTemp: $("#cpuTemp"),
    cpuCores: $("#cpuCores"),
    cpuLoad: $("#cpuLoad"),

    ramRing: $("#ramRing"),
    ramValue: $("#ramValue"),
    ramTotalChip: $("#ramTotalChip"),
    ramUsed: $("#ramUsed"),
    ramFree: $("#ramFree"),

    netIface: $("#netIface"),
    netDownValue: $("#netDownValue"),
    netDownUnit: $("#netDownUnit"),
    netUpValue: $("#netUpValue"),
    netUpUnit: $("#netUpUnit"),
    netDownBar: $("#netDownBar"),
    netUpBar: $("#netUpBar"),
    rxLine: $("#rxLine"),
    txLine: $("#txLine"),
    rxArea: $("#rxArea"),
    txArea: $("#txArea"),
    netPeak: $("#netPeak"),
    netTotal: $("#netTotal"),

    deck: $("#deck"),
    toasts: $("#toasts"),

    sysHost: $("#sysHost"),
    sysOs: $("#sysOs"),
    sysUptime: $("#sysUptime"),
    sysLoad: $("#sysLoad"),
    sysPing: $("#sysPing"),
    sysRtt: $("#sysRtt"),
    deckVersion: $("#deckVersion"),
    playIcon: $("#playIcon"),
    playSub: $("#playSub"),
    gameSub: $("#gameSub"),

    lock: $("#lock"),
    lockForm: $("#lockForm"),
    tokenInput: $("#tokenInput"),
    lockError: $("#lockError"),
  };

  const state = {
    token: "",
    authRequired: false,
    locked: false,
    booted: false,
    ws: null,
    wsOpen: false,
    reconnectAttempt: 0,
    reconnectTimer: null,
    pollTimer: null,
    pingTimer: null,
    pending: new Map(),
    metrics: null,
    history: Array.from({ length: HISTORY_LEN }, () => ({ t: 0, rx: 0, tx: 0 })),
    chartMax: 1024,
    host: "",
    version: "",
    toggles: { mic: false, deafen: false, playing: true, gameMode: false },
  };

  const dateFormatter = new Intl.DateTimeFormat(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
  });

  // -------------------------------------------------------------------------
  // Boot
  // -------------------------------------------------------------------------

  async function boot() {
    state.token = readToken();
    state.booted = true;

    initRings();
    bindDeck();
    bindLock();
    bindNetworkEvents();
    updateClock();
    setInterval(updateClock, 1000);
    renderNetwork();
    setConnection("connecting");

    const health = await fetch("/api/health", { cache: "no-store" })
      .then((response) => (response.ok ? response.json() : null))
      .catch(() => null);

    if (health) {
      state.authRequired = Boolean(health.auth_required);
      if (health.version) {
        state.version = health.version;
        setText(els.deckVersion, `v${health.version}`);
      }
      if (health.host) {
        state.host = health.host;
        setText(els.brandHost, health.host);
        setText(els.sysHost, health.host);
      }
    }

    if (state.authRequired) {
      if (!state.token) {
        showLock();
        setConnection("locked");
        return;
      }
      const valid = await verifyToken();
      if (valid === false) {
        showLock("Token rejected. Check the server console.");
        setConnection("locked");
        return;
      }
    }

    connect();
  }

  function readToken() {
    let token = "";
    try {
      token = localStorage.getItem(TOKEN_KEY) || "";
    } catch {
      token = "";
    }

    const url = new URL(window.location.href);
    const urlToken = url.searchParams.get("token");
    if (urlToken) {
      token = urlToken;
      storeToken(urlToken);
      url.searchParams.delete("token");
      history.replaceState(
        null,
        "",
        `${url.pathname}${url.search}${url.hash}`,
      );
    }

    return token.trim();
  }

  function storeToken(token) {
    try {
      if (token) localStorage.setItem(TOKEN_KEY, token);
      else localStorage.removeItem(TOKEN_KEY);
    } catch {
      /* private mode: keep it in memory only */
    }
  }

  async function verifyToken() {
    try {
      const response = await fetch("/api/auth", {
        headers: authHeaders(),
        cache: "no-store",
      });
      if (response.status === 401) return false;
      if (response.ok) return true;
      return null;
    } catch {
      return null;
    }
  }

  function authHeaders() {
    return state.token ? { "x-liquid-token": state.token } : {};
  }

  // -------------------------------------------------------------------------
  // Clock
  // -------------------------------------------------------------------------

  function updateClock() {
    const now = new Date();
    const hours = String(now.getHours()).padStart(2, "0");
    const minutes = String(now.getMinutes()).padStart(2, "0");
    const seconds = String(now.getSeconds()).padStart(2, "0");

    setText(els.clockTime, `${hours}:${minutes}`);
    setText(els.clockSeconds, seconds);
    setText(els.clockDate, dateFormatter.format(now).toUpperCase());
  }

  // -------------------------------------------------------------------------
  // WebSocket
  // -------------------------------------------------------------------------

  function connect() {
    if (state.locked || !state.booted) return;
    if (
      state.ws &&
      (state.ws.readyState === WebSocket.OPEN ||
        state.ws.readyState === WebSocket.CONNECTING)
    ) {
      return;
    }

    clearTimeout(state.reconnectTimer);
    setConnection("connecting");

    const scheme = window.location.protocol === "https:" ? "wss" : "ws";
    const token = state.token ? `?token=${encodeURIComponent(state.token)}` : "";
    const url = `${scheme}://${window.location.host}/ws${token}`;

    let ws;
    try {
      ws = new WebSocket(url);
    } catch {
      scheduleReconnect();
      return;
    }

    state.ws = ws;
    let opened = false;

    ws.addEventListener("open", () => {
      if (state.ws !== ws) return;
      opened = true;
      state.wsOpen = true;
      state.reconnectAttempt = 0;
      setConnection("live");
      stopPolling();
      startPing();
    });

    ws.addEventListener("message", (event) => {
      let message;
      try {
        message = JSON.parse(event.data);
      } catch {
        return;
      }
      handleMessage(message);
    });

    ws.addEventListener("close", () => {
      if (state.ws !== ws) return;
      state.ws = null;
      state.wsOpen = false;
      stopPing();
      failPending("Connection lost.");
      if (state.locked) return;

      // A failed handshake usually means the token was revoked. Verify once,
      // but only lock on an explicit 401, never on a network error.
      if (!opened && state.authRequired && state.token) {
        verifyToken().then((valid) => {
          if (valid === false) {
            showLock("Token rejected. Check the server console.");
          }
        });
      }

      setConnection("offline");
      scheduleReconnect();
      startPolling();
    });

    ws.addEventListener("error", () => {
      try {
        ws.close();
      } catch {
        /* already closing */
      }
    });
  }

  function scheduleReconnect() {
    if (state.locked) return;
    clearTimeout(state.reconnectTimer);
    const attempt = Math.min(state.reconnectAttempt, 6);
    state.reconnectAttempt += 1;
    const base = Math.min(MAX_RECONNECT_MS, 450 * 2 ** attempt);
    const delay = base + Math.random() * 250;
    state.reconnectTimer = setTimeout(connect, delay);
  }

  function handleMessage(message) {
    if (!message || typeof message !== "object") return;

    switch (message.type) {
      case "hello":
        state.host = message.host || state.host;
        state.version = message.version || state.version;
        state.authRequired = Boolean(message.auth_required);

        if (state.host) {
          setText(els.brandHost, state.host);
          setText(els.sysHost, state.host);
          document.title = `Liquid Deck · ${state.host}`;
        }
        if (message.os) setText(els.sysOs, message.os);
        if (state.version) setText(els.deckVersion, `v${state.version}`);

        setHistory(message.history);
        if (message.metrics) applyMetrics(message.metrics, { push: false });
        else renderNetwork();
        break;

      case "metrics":
        applyMetrics(message);
        break;

      case "pong": {
        const sent = Number(message.t);
        if (Number.isFinite(sent) && sent > 0) {
          const rtt = Math.max(0, Date.now() - sent);
          setText(els.sysRtt, `${rtt} ms`);
        }
        break;
      }

      case "action_result":
        resolvePending(message);
        break;

      case "error":
        toast(message.message || "Server error.", "error");
        break;

      default:
        break;
    }
  }

  function startPing() {
    stopPing();
    const send = () => {
      if (!state.wsOpen || !state.ws || state.ws.readyState !== WebSocket.OPEN) return;
      try {
        state.ws.send(JSON.stringify({ type: "ping", t: Date.now() }));
      } catch {
        /* socket closed between checks */
      }
    };
    setTimeout(send, 350);
    state.pingTimer = setInterval(send, 5000);
  }

  function stopPing() {
    if (state.pingTimer) {
      clearInterval(state.pingTimer);
      state.pingTimer = null;
    }
  }

  // -------------------------------------------------------------------------
  // Polling fallback
  // -------------------------------------------------------------------------

  function startPolling() {
    if (state.locked || state.pollTimer) return;
    state.pollTimer = setInterval(pollMetrics, 2000);
    pollMetrics();
  }

  function stopPolling() {
    if (state.pollTimer) {
      clearInterval(state.pollTimer);
      state.pollTimer = null;
    }
  }

  async function pollMetrics() {
    if (state.locked) return;
    try {
      const response = await fetch("/api/metrics", {
        headers: authHeaders(),
        cache: "no-store",
      });
      if (response.status === 401) {
        showLock("Token rejected. Check the server console.");
        return;
      }
      if (!response.ok) {
        if (!state.wsOpen) setConnection("offline");
        return;
      }
      const data = await response.json();
      if (data && data.type === "metrics") {
        if (!state.wsOpen) setConnection("polling");
        applyMetrics(data);
      }
    } catch {
      if (!state.wsOpen) setConnection("offline");
    }
  }

  // -------------------------------------------------------------------------
  // Metrics rendering
  // -------------------------------------------------------------------------

  function applyMetrics(metrics, options = {}) {
    if (!metrics || typeof metrics !== "object") return;
    state.metrics = metrics;

    const cpu = clamp(Number(metrics.cpu) || 0, 0, 100);
    const ram = clamp(Number(metrics.ram_pct) || 0, 0, 100);

    setRing(els.cpuRing, cpu);
    setRing(els.ramRing, ram);
    setText(els.cpuValue, String(Math.round(cpu)));
    setText(els.ramValue, String(Math.round(ram)));

    const temperature = Number(metrics.temp);
    setText(
      els.cpuTemp,
      Number.isFinite(temperature) && temperature > 0
        ? `${Math.round(temperature)}°`
        : "—",
    );

    setText(
      els.cpuCores,
      Number.isFinite(Number(metrics.cpu_cores))
        ? `${metrics.cpu_cores} cores`
        : "— cores",
    );
    setText(els.cpuLoad, `load ${formatLoad(metrics.load && metrics.load[0])}`);

    const ramTotal = Number(metrics.ram_total) || 0;
    const ramUsed = Number(metrics.ram_used) || 0;
    const ramFree = Math.max(0, ramTotal - ramUsed);
    setText(els.ramTotalChip, formatBytes(ramTotal));
    setText(els.ramUsed, `used ${formatBytes(ramUsed)}`);
    setText(els.ramFree, `free ${formatBytes(ramFree)}`);

    const rx = Math.max(0, Number(metrics.net_rx) || 0);
    const tx = Math.max(0, Number(metrics.net_tx) || 0);
    const down = formatRate(rx);
    const up = formatRate(tx);
    setText(els.netDownValue, down.value);
    setText(els.netDownUnit, down.unit);
    setText(els.netUpValue, up.value);
    setText(els.netUpUnit, up.unit);
    setText(els.netIface, metrics.iface || "—");
    setText(
      els.netTotal,
      `↓ ${formatBytes(metrics.net_rx_total)} · ↑ ${formatBytes(
        metrics.net_tx_total,
      )} total`,
    );

    if (options.push !== false) {
      pushHistory({
        t: Number(metrics.ts) || Date.now(),
        rx,
        tx,
      });
    }

    renderNetwork(rx, tx);

    const ping = metrics.ping;
    if (typeof ping === "number" && Number.isFinite(ping) && ping >= 0) {
      const display = ping < 10 ? ping.toFixed(1) : String(Math.round(ping));
      setText(els.pingValue, display);
      setText(els.sysPing, `${display} ms`);
      els.pingPill.dataset.quality =
        ping < 60 ? "good" : ping < 150 ? "ok" : "bad";
    } else {
      setText(els.pingValue, "--");
      setText(els.sysPing, "—");
      els.pingPill.dataset.quality = "unknown";
    }

    setText(els.sysUptime, formatUptime(Number(metrics.uptime) || 0));
    setText(els.sysLoad, formatLoadTriple(metrics.load));
    if (metrics.host) {
      setText(els.sysHost, metrics.host);
      if (!state.host) {
        state.host = metrics.host;
        setText(els.brandHost, metrics.host);
      }
    }
    if (metrics.os) setText(els.sysOs, metrics.os);
  }

  function pushHistory(point) {
    state.history.push(point);
    while (state.history.length > HISTORY_LEN) state.history.shift();
  }

  function setHistory(points) {
    const list = Array.isArray(points) ? points.slice(-HISTORY_LEN) : [];
    const padding = Math.max(0, HISTORY_LEN - list.length);
    const blanks = Array.from({ length: padding }, () => ({ t: 0, rx: 0, tx: 0 }));
    state.history = blanks.concat(
      list.map((point) => ({
        t: Number(point.t) || 0,
        rx: Math.max(0, Number(point.rx) || 0),
        tx: Math.max(0, Number(point.tx) || 0),
      })),
    );
    state.chartMax = 1024;
  }

  function initRings() {
    for (const ring of [els.cpuRing, els.ramRing]) {
      if (!ring) continue;
      ring.style.strokeDasharray = String(RING_CIRCUMFERENCE);
      ring.style.strokeDashoffset = String(RING_CIRCUMFERENCE);
    }
  }

  function setRing(element, percent) {
    if (!element) return;
    const value = clamp(percent, 0, 100);
    const offset = RING_CIRCUMFERENCE * (1 - value / 100);
    element.style.strokeDashoffset = String(offset);
    element.classList.toggle("is-hot", value >= 85);
  }

  function renderNetwork(currentRx, currentTx) {
    const rxValues = state.history.map((point) => Math.max(0, point.rx));
    const txValues = state.history.map((point) => Math.max(0, point.tx));
    const peak = Math.max(1, ...rxValues, ...txValues);

    const target = Math.max(peak * 1.2, 1024);
    state.chartMax = Math.max(target, state.chartMax * 0.9);
    const scale = state.chartMax;

    if (els.rxLine) els.rxLine.setAttribute("d", linePath(rxValues, scale));
    if (els.txLine) els.txLine.setAttribute("d", linePath(txValues, scale));
    if (els.rxArea) els.rxArea.setAttribute("d", areaPath(rxValues, scale));
    if (els.txArea) els.txArea.setAttribute("d", areaPath(txValues, scale));

    if (els.netDownBar) {
      const width = clamp(((currentRx || 0) / scale) * 100, 0, 100);
      els.netDownBar.style.width = `${width}%`;
    }
    if (els.netUpBar) {
      const width = clamp(((currentTx || 0) / scale) * 100, 0, 100);
      els.netUpBar.style.width = `${width}%`;
    }

    const peakLabel = formatRate(scale);
    setText(els.netPeak, `peak ${peakLabel.value} ${peakLabel.unit}`);
  }

  function linePath(values, maximum) {
    const width = 360;
    const height = 110;
    const top = 9;
    const bottom = 10;
    const step = width / Math.max(1, values.length - 1);
    const y = (value) =>
      height - bottom - (Math.max(0, value) / maximum) * (height - top - bottom);

    let path = `M0,${y(values[0]).toFixed(2)}`;
    for (let index = 1; index < values.length; index += 1) {
      const x0 = (index - 1) * step;
      const x1 = index * step;
      const y0 = y(values[index - 1]);
      const y1 = y(values[index]);
      const middle = (x0 + x1) / 2;
      path += ` C${middle.toFixed(2)},${y0.toFixed(2)} ${middle.toFixed(
        2,
      )},${y1.toFixed(2)} ${x1.toFixed(2)},${y1.toFixed(2)}`;
    }
    return path;
  }

  function areaPath(values, maximum) {
    return `${linePath(values, maximum)} L360,110 L0,110 Z`;
  }

  // -------------------------------------------------------------------------
  // Connection UI
  // -------------------------------------------------------------------------

  function setConnection(mode) {
    if (!els.connPill) return;
    const labels = {
      live: "Live",
      connecting: "Linking",
      offline: "Offline",
      polling: "Polling",
      locked: "Locked",
    };
    els.connPill.dataset.state = mode;
    setText(els.connLabel, labels[mode] || mode);

    const showBanner = mode === "offline" || mode === "polling";
    document.body.classList.toggle("is-offline", showBanner);
    if (els.reconnectBanner) {
      els.reconnectBanner.classList.toggle("is-visible", showBanner);
    }
    if (showBanner) {
      setText(
        els.reconnectText,
        mode === "polling"
          ? "Live link lost — polling metrics…"
          : "Connection lost — reconnecting…",
      );
    }
  }

  // -------------------------------------------------------------------------
  // Action deck
  // -------------------------------------------------------------------------

  function bindDeck() {
    for (const button of $$(".key", els.deck)) {
      button.addEventListener("pointerdown", (event) => {
        if (event.pointerType === "mouse" && event.button !== 0) return;
        button.classList.add("pressed");
        vibrate(10);
      });

      for (const eventName of ["pointerup", "pointercancel", "pointerleave"]) {
        button.addEventListener(eventName, () => {
          button.classList.remove("pressed");
        });
      }

      button.addEventListener("click", () => {
        button.classList.remove("pressed");
        trigger(button);
      });

      button.addEventListener("contextmenu", (event) => event.preventDefault());
    }
  }

  function payloadFor(button) {
    const action = button.dataset.action;
    switch (action) {
      case "media":
        return { action, command: button.dataset.value || "play_pause" };
      case "volume":
        return { action, direction: button.dataset.value || "up", step: 5 };
      case "game_mode":
        return { action, enabled: !state.toggles.gameMode };
      case "launch":
        return { action, target: button.dataset.value || "terminal" };
      case "key":
        return { action, combo: button.dataset.value || "" };
      case "script":
        return { action, name: button.dataset.value || "" };
      default:
        return { action };
    }
  }

  async function trigger(button) {
    if (button.classList.contains("busy")) return;
    button.classList.add("busy");

    try {
      const result = await sendAction(payloadFor(button));
      handleActionResult(result, button);
    } finally {
      button.classList.remove("busy");
    }
  }

  async function sendAction(payload) {
    if (state.wsOpen && state.ws && state.ws.readyState === WebSocket.OPEN) {
      const id = makeId();
      const promise = new Promise((resolve) => {
        const timer = setTimeout(() => {
          state.pending.delete(id);
          resolve({ ok: false, message: "No response from the deck." });
        }, ACTION_TIMEOUT_MS);
        state.pending.set(id, { resolve, timer });
      });

      try {
        state.ws.send(JSON.stringify({ type: "action", ...payload, request_id: id }));
      } catch {
        state.pending.delete(id);
        return postAction(payload);
      }
      return promise;
    }

    return postAction(payload);
  }

  async function postAction(payload) {
    try {
      const response = await fetch("/api/action", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          ...authHeaders(),
        },
        body: JSON.stringify(payload),
      });

      if (response.status === 401) {
        showLock("Token rejected. Check the server console.");
        return { ok: false, message: "Unauthorized." };
      }

      const data = await response.json().catch(() => null);
      if (!data) {
        return { ok: false, message: `Action failed (HTTP ${response.status}).` };
      }
      return data;
    } catch {
      return { ok: false, message: "Deck unreachable." };
    }
  }

  function resolvePending(message) {
    const id = message.request_id;
    if (!id) return;
    const entry = state.pending.get(id);
    if (!entry) return;
    clearTimeout(entry.timer);
    state.pending.delete(id);
    entry.resolve(message);
  }

  function failPending(reason) {
    for (const [, entry] of state.pending) {
      clearTimeout(entry.timer);
      entry.resolve({ ok: false, message: reason });
    }
    state.pending.clear();
  }

  function makeId() {
    if (window.crypto && typeof window.crypto.randomUUID === "function") {
      return window.crypto.randomUUID();
    }
    return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  }

  function handleActionResult(result, button) {
    if (!result || typeof result !== "object") {
      toast("No response from the deck.", "error");
      return;
    }

    if (!result.ok) {
      toast(result.message || "Action failed.", "error");
      vibrate([18, 40, 18]);
      if (/unauthorized/i.test(result.message || "")) {
        showLock("Token rejected. Check the server console.");
      }
      return;
    }

    const action = (button && button.dataset.action) || result.action;
    vibrate(8);

    switch (action) {
      case "discord_mute": {
        state.toggles.mic = !state.toggles.mic;
        setActive(button, state.toggles.mic, state.toggles.mic ? "Muted" : "Ctrl ⇧ M");
        toast(
          state.toggles.mic ? "Mic mute sent" : "Mic unmute sent",
          "success",
        );
        break;
      }

      case "discord_deafen": {
        state.toggles.deafen = !state.toggles.deafen;
        setActive(
          button,
          state.toggles.deafen,
          state.toggles.deafen ? "Deafened" : "Ctrl ⇧ D",
        );
        toast(
          state.toggles.deafen ? "Deafen sent" : "Undeafen sent",
          "success",
        );
        break;
      }

      case "media": {
        const command = (button && button.dataset.value) || "";
        if (command === "play_pause") {
          state.toggles.playing = !state.toggles.playing;
          setActive(button, !state.toggles.playing, state.toggles.playing ? "Playing" : "Paused");
          if (els.playIcon) {
            els.playIcon.setAttribute(
              "href",
              state.toggles.playing ? "#icon-pause" : "#icon-play",
            );
          }
          toast(state.toggles.playing ? "Playing" : "Paused", "success");
        } else if (command === "next") {
          toast("Next track", "success");
        } else {
          toast(result.message || "Media command sent", "success");
        }
        break;
      }

      case "volume":
        toast(
          button && button.dataset.value === "down"
            ? "Volume down"
            : "Volume up",
          "success",
        );
        break;

      case "terminal":
        toast("Terminal launched", "success");
        break;

      case "game_mode": {
        state.toggles.gameMode = !state.toggles.gameMode;
        setActive(
          button,
          state.toggles.gameMode,
          state.toggles.gameMode ? "Active" : "Standby",
        );
        toast(
          state.toggles.gameMode ? "Game mode on" : "Game mode off",
          "success",
        );
        break;
      }

      case "script":
        toast(result.message || "Script finished", "success");
        break;

      case "key":
        toast("Shortcut sent", "success");
        break;

      default:
        toast(result.message && result.message !== "ok" ? result.message : "Done", "success");
        break;
    }
  }

  function setActive(button, active, subtitle) {
    if (!button) return;
    button.classList.toggle("is-active", Boolean(active));
    button.setAttribute("aria-pressed", String(Boolean(active)));
    if (subtitle) {
      const small = button.querySelector("small");
      if (small) setText(small, subtitle);
    }
  }

  // -------------------------------------------------------------------------
  // Lock screen
  // -------------------------------------------------------------------------

  function bindLock() {
    if (!els.lockForm) return;
    els.lockForm.addEventListener("submit", async (event) => {
      event.preventDefault();
      const token = els.tokenInput.value.trim();
      if (!token) {
        showLockError("Enter the access token.");
        return;
      }

      state.token = token;
      const valid = await verifyToken();
      if (valid !== true) {
        state.token = "";
        showLockError(
          valid === false
            ? "Token rejected. Check the server console."
            : "Server unreachable. Try again.",
        );
        vibrate([18, 40, 18]);
        return;
      }

      storeToken(token);
      hideLock();
      setConnection("connecting");
      connect();
    });

    els.tokenInput.addEventListener("input", () => {
      if (els.lockError) els.lockError.hidden = true;
    });
  }

  function showLock(message) {
    state.locked = true;
    clearTimeout(state.reconnectTimer);
    stopPolling();
    stopPing();
    failPending("Deck locked.");

    if (state.ws) {
      const socket = state.ws;
      state.ws = null;
      state.wsOpen = false;
      try {
        socket.close();
      } catch {
        /* already closed */
      }
    }

    if (!els.lock) return;
    els.lock.hidden = false;
    document.body.classList.add("is-locked");
    if (els.lockError) {
      els.lockError.hidden = !message;
      els.lockError.textContent = message || "";
    }
    setConnection("locked");

    if (window.matchMedia("(pointer: fine)").matches) {
      setTimeout(() => els.tokenInput && els.tokenInput.focus(), 120);
    }
  }

  function hideLock() {
    state.locked = false;
    if (els.lock) els.lock.hidden = true;
    document.body.classList.remove("is-locked");
    if (els.lockError) els.lockError.hidden = true;
    if (els.tokenInput) els.tokenInput.value = "";
  }

  function showLockError(message) {
    if (!els.lockError) return;
    els.lockError.textContent = message;
    els.lockError.hidden = false;
  }

  // -------------------------------------------------------------------------
  // Toasts & haptics
  // -------------------------------------------------------------------------

  function toast(message, type = "info") {
    if (!els.toasts) return;
    const node = document.createElement("div");
    node.className = `toast toast-${type}`;

    const icon = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    icon.setAttribute("class", "toast-icon");
    icon.setAttribute("aria-hidden", "true");
    const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
    use.setAttribute(
      "href",
      type === "error" ? "#icon-alert" : type === "success" ? "#icon-check" : "#icon-bolt",
    );
    icon.appendChild(use);

    const text = document.createElement("span");
    text.textContent = message;

    node.append(icon, text);
    els.toasts.appendChild(node);
    while (els.toasts.children.length > 3) {
      els.toasts.firstElementChild.remove();
    }

    requestAnimationFrame(() => node.classList.add("is-in"));
    setTimeout(() => {
      node.classList.remove("is-in");
      node.classList.add("is-out");
      setTimeout(() => node.remove(), 300);
    }, 2800);
  }

  function vibrate(pattern) {
    if (typeof navigator.vibrate !== "function") return;
    try {
      navigator.vibrate(pattern);
    } catch {
      /* vibration unsupported or blocked */
    }
  }

  // -------------------------------------------------------------------------
  // Network lifecycle & utilities
  // -------------------------------------------------------------------------

  function bindNetworkEvents() {
    window.addEventListener("online", () => {
      if (!state.locked) connect();
    });
    window.addEventListener("offline", () => {
      if (!state.locked) setConnection("offline");
    });
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "visible" && !state.locked) {
        if (!state.wsOpen) connect();
      }
    });
  }

  function setText(element, value) {
    if (!element) return;
    const next = value == null ? "" : String(value);
    if (element.textContent !== next) element.textContent = next;
  }

  function clamp(value, min, max) {
    return Math.min(max, Math.max(min, value));
  }

  function formatRate(bytesPerSecond) {
    const value = Number(bytesPerSecond);
    if (!Number.isFinite(value) || value <= 0) {
      return { value: "0", unit: "B/s" };
    }

    const units = ["B/s", "KB/s", "MB/s", "GB/s", "TB/s"];
    let scaled = value;
    let index = 0;
    while (scaled >= 1024 && index < units.length - 1) {
      scaled /= 1024;
      index += 1;
    }

    const digits = index === 0 ? 0 : scaled >= 100 ? 0 : scaled >= 10 ? 1 : 1;
    return { value: scaled.toFixed(digits), unit: units[index] };
  }

  function formatBytes(bytes) {
    const value = Number(bytes);
    if (!Number.isFinite(value) || value <= 0) return "0 B";

    const units = ["B", "KB", "MB", "GB", "TB"];
    let scaled = value;
    let index = 0;
    while (scaled >= 1024 && index < units.length - 1) {
      scaled /= 1024;
      index += 1;
    }

    const digits = index === 0 ? 0 : scaled >= 100 ? 0 : scaled >= 10 ? 1 : 1;
    return `${scaled.toFixed(digits)} ${units[index]}`;
  }

  function formatUptime(seconds) {
    if (!Number.isFinite(seconds) || seconds <= 0) return "—";
    const days = Math.floor(seconds / 86400);
    const hours = Math.floor((seconds % 86400) / 3600);
    const minutes = Math.floor((seconds % 3600) / 60);
    if (days > 0) return `${days}d ${hours}h`;
    if (hours > 0) return `${hours}h ${minutes}m`;
    return `${minutes}m`;
  }

  function formatLoad(value) {
    const number = Number(value);
    return Number.isFinite(number) ? number.toFixed(2) : "—";
  }

  function formatLoadTriple(load) {
    if (!Array.isArray(load) || load.length < 3) return "—";
    return load.slice(0, 3).map(formatLoad).join(" / ");
  }

  boot();
})();
