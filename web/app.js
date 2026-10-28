/* Liquid Deck — vanilla JS client
   WebSocket telemetry, SVG gauges, configurable boards, deck editor and mixer.
   No dependencies. */

(() => {
  "use strict";

  const HISTORY_LEN = 60;
  const RING_CIRCUMFERENCE = 2 * Math.PI * 54;
  const TOKEN_KEY = "liquid-deck.token";
  const MAX_RECONNECT_MS = 10_000;
  const ACTION_TIMEOUT_MS = 12_000;
  const LONG_PRESS_MS = 550;

  const MAX_BUTTONS = 32;
  const MAX_BOARDS = 16;
  const MIN_COLUMNS = 2;
  const MAX_COLUMNS = 8;

  const COLORS = {
    blue: "#0a84ff",
    cyan: "#64d2ff",
    violet: "#bf5af2",
    pink: "#ff375f",
    green: "#30d158",
    orange: "#ff9f0a",
    yellow: "#ffd60a",
    red: "#ff453a",
    slate: "#c7c7cc",
  };
  const COLOR_NAMES = Object.keys(COLORS);

  const ACTIONS = [
    "discord_mute",
    "discord_deafen",
    "media",
    "volume",
    "mute",
    "key",
    "script",
    "launch",
    "terminal",
    "game_mode",
    "url",
    "sound",
    "obs",
    "audio",
    "board",
    "panel",
    "none",
  ];

  const ACTION_LABELS = {
    discord_mute: "Discord · mic mute",
    discord_deafen: "Discord · deafen",
    media: "Media control",
    volume: "System volume",
    mute: "System mute",
    key: "Keyboard shortcut",
    script: "Run script",
    launch: "Launch app",
    terminal: "Open terminal",
    game_mode: "Game mode",
    url: "Open URL",
    sound: "Play sound",
    obs: "OBS WebSocket",
    audio: "Audio mixer",
    board: "Switch board",
    panel: "Open panel",
    none: "No action",
  };

  const MEDIA_COMMANDS = ["play_pause", "next", "previous", "stop"];
  const LAUNCH_TARGETS = [
    "terminal",
    "browser",
    "files",
    "calculator",
    "editor",
  ];
  const PANELS = ["editor", "settings", "audio"];
  const OBS_OPS = [
    "scene",
    "record_start",
    "record_stop",
    "record_toggle",
    "stream_start",
    "stream_stop",
    "stream_toggle",
    "mute_toggle",
    "mute_on",
    "mute_off",
    "replay_start",
    "replay_stop",
    "replay_save",
    "scene_collection",
    "test",
  ];
  const AUDIO_OPS = ["output", "volume", "mute", "app_volume", "app_mute"];

  const ICON_NAMES = [
    "bolt",
    "mic",
    "mic-off",
    "headphones",
    "headphones-off",
    "play",
    "pause",
    "previous",
    "next",
    "volume-up",
    "volume-down",
    "volume-x",
    "speaker",
    "terminal",
    "gamepad",
    "record",
    "stream",
    "camera",
    "folder",
    "globe",
    "chat",
    "music",
    "code",
    "grid",
    "sliders",
    "settings",
    "sound",
    "obs",
    "github",
    "book",
    "cpu",
    "memory",
    "thermometer",
    "wifi",
    "activity",
    "play-circle",
    "copy",
    "refresh",
    "lock",
    "edit",
    "arrow-up",
    "arrow-down",
  ];

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
    boardTabs: $("#boardTabs"),
    editBtn: $("#editBtn"),
    audioBtn: $("#audioBtn"),
    deckNote: $("#deckNote"),
    toasts: $("#toasts"),

    sysHost: $("#sysHost"),
    sysOs: $("#sysOs"),
    sysUptime: $("#sysUptime"),
    sysLoad: $("#sysLoad"),
    sysPing: $("#sysPing"),
    sysRtt: $("#sysRtt"),
    deckVersion: $("#deckVersion"),

    editorSheet: $("#editorSheet"),
    editorBody: $("#editorBody"),
    editorTabs: $("#editorTabs"),
    editorClose: $("#editorClose"),
    editorCancel: $("#editorCancel"),
    editorSave: $("#editorSave"),
    editorReset: $("#editorReset"),
    dirtyBadge: $("#dirtyBadge"),

    audioSheet: $("#audioSheet"),
    audioBody: $("#audioBody"),
    audioRefresh: $("#audioRefresh"),
    audioClose: $("#audioClose"),

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
    scriptsEnabled: false,

    config: null,
    deckState: { toggles: {}, active_board: "", updated_at: 0 },
    activeBoard: "",
    boardStack: [],
    sounds: [],
    soundsEnabled: true,
    previewAudio: null,
    sheet: null,

    editor: {
      draft: null,
      tab: "boards",
      boardId: "",
      buttonId: "",
      jsonText: null,
      jsonError: "",
      saving: false,
    },
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
    bindBoards();
    bindEditor();
    bindAudio();
    bindSheets();
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
      state.scriptsEnabled = Boolean(health.scripts);
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

    await loadDeck();
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
        state.scriptsEnabled = Boolean(message.scripts);
        if (Array.isArray(message.sounds)) state.sounds = message.sounds;
        if (typeof message.sounds_enabled === "boolean") {
          state.soundsEnabled = message.sounds_enabled;
        }

        if (state.host) {
          setText(els.brandHost, state.host);
          setText(els.sysHost, state.host);
          document.title = `Liquid Deck · ${state.host}`;
        }
        if (message.os) setText(els.sysOs, message.os);
        if (state.version) setText(els.deckVersion, `v${state.version}`);

        applyDeckPayload(message.config, message.state);
        setHistory(message.history);
        if (message.metrics) applyMetrics(message.metrics, { push: false });
        else renderNetwork();
        break;

      case "config":
        applyDeckPayload(message.config, message.state);
        settleExternalConfig(message.config);
        break;

      case "deck_state":
        applyDeckState(message.state);
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
  // Deck configuration
  // -------------------------------------------------------------------------

  async function apiRequest(path, options = {}) {
    const headers = Object.assign({}, options.headers || {}, authHeaders());
    if (
      options.body != null &&
      typeof options.body === "string" &&
      !headers["content-type"]
    ) {
      headers["content-type"] = "application/json";
    }
    const response = await fetch(path, {
      ...options,
      headers,
      cache: "no-store",
    });
    if (response.status === 401) {
      showLock("Token rejected. Check the server console.");
    }
    return response;
  }

  async function loadDeck() {
    try {
      const response = await apiRequest("/api/config");
      if (!response.ok) return;
      const data = await response.json();
      if (!data || !data.ok) return;
      if (Array.isArray(data.sounds)) state.sounds = data.sounds;
      if (typeof data.sounds_enabled === "boolean") {
        state.soundsEnabled = data.sounds_enabled;
      }
      applyDeckPayload(data.config, data.state);
    } catch {
      /* the WebSocket hello will carry the deck once it connects */
    }
  }

  function normalizeDeckState(input) {
    const source = input && typeof input === "object" ? input : {};
    const toggles =
      source.toggles && typeof source.toggles === "object" ? source.toggles : {};
    return {
      toggles: { ...toggles },
      active_board: typeof source.active_board === "string" ? source.active_board : "",
      updated_at: Number(source.updated_at) || 0,
    };
  }

  function applyDeckPayload(config, deckState) {
    if (!config || !Array.isArray(config.boards) || !config.boards.length) return;
    state.config = config;
    state.deckState = normalizeDeckState(deckState);

    const ids = new Set(config.boards.map((board) => board.id));
    if (!ids.has(state.activeBoard)) {
      const preferred = state.deckState.active_board;
      const fallback =
        config.settings && ids.has(config.settings.default_board)
          ? config.settings.default_board
          : config.boards[0].id;
      state.activeBoard = ids.has(preferred) ? preferred : fallback;
      state.boardStack = [];
    }

    renderBoardTabs();
    renderDeck();
  }

  function applyDeckState(deckState) {
    const next = normalizeDeckState(deckState);
    state.deckState = next;

    const board = findBoard(next.active_board);
    if (board && next.active_board !== state.activeBoard) {
      state.activeBoard = next.active_board;
      state.boardStack = [];
      renderBoardTabs();
      renderDeck();
      return;
    }

    refreshToggleStates();
  }

  function settleExternalConfig() {
    if (!state.editor.draft || state.sheet !== "editor") return;
    if (!isEditorDirty()) renderEditor();
  }

  function findBoard(id) {
    if (!state.config || !Array.isArray(state.config.boards)) return null;
    return state.config.boards.find((board) => board.id === id) || null;
  }

  function currentBoard() {
    if (!state.config || !Array.isArray(state.config.boards)) return null;
    return findBoard(state.activeBoard) || state.config.boards[0] || null;
  }

  function isActive(boardId, buttonId) {
    const key = `${boardId}/${buttonId}`;
    return Boolean(state.deckState.toggles && state.deckState.toggles[key]);
  }

  function setLocalToggle(boardId, buttonId, active) {
    if (!state.deckState.toggles) state.deckState.toggles = {};
    state.deckState.toggles[`${boardId}/${buttonId}`] = Boolean(active);
  }

  // -------------------------------------------------------------------------
  // Deck rendering
  // -------------------------------------------------------------------------

  function renderBoardTabs() {
    if (!els.boardTabs) return;
    const boards = state.config && Array.isArray(state.config.boards)
      ? state.config.boards
      : [];

    const parts = [];
    if (state.boardStack.length) {
      parts.push(
        `<button class="board-back" type="button" data-board-back aria-label="Back to previous board">
          ${iconMarkup("arrow-left")}<span>Back</span>
        </button>`,
      );
    }

    for (const board of boards) {
      const active = board.id === state.activeBoard;
      const tone = boardTabVars(board.accent);
      parts.push(
        `<button class="board-tab${active ? " is-active" : ""}" type="button"
          role="tab" aria-selected="${active}" data-board="${esc(board.id)}"
          style="${tone}">
          <span class="tab-dot" aria-hidden="true"></span>
          <span>${esc(board.name)}</span>
        </button>`,
      );
    }

    els.boardTabs.innerHTML = parts.join("");
    if (els.deckNote) {
      setText(els.deckNote, state.boardStack.length ? "Nested board · Back to return" : "Tap to trigger · long-press to edit");
    }
  }

  function keyMarkup(board, button, options = {}) {
    const active = options.preview ? false : isActive(board.id, button.id);
    const icon =
      active && button.active_icon ? button.active_icon : button.icon || "bolt";
    const label =
      active && button.active_label ? button.active_label : button.label;
    const showSubtitle =
      options.preview || !state.config || !state.config.settings
        ? true
        : state.config.settings.show_subtitles !== false;
    const columns = Math.max(MIN_COLUMNS, Number(board.columns) || 4);
    const span = clamp(Number(button.span) || 1, 1, Math.min(4, columns));
    const tone = toneVars(button.color);
    const spanStyle = options.preview ? "" : `grid-column:span ${span};`;

    const attrs = options.preview
      ? 'type="button" tabindex="-1"'
      : `type="button" data-board="${esc(board.id)}" data-button="${esc(
          button.id,
        )}" data-action="${esc(button.action)}" aria-pressed="${active}"`;
    const subtitle =
      showSubtitle && button.subtitle
        ? `<small>${esc(button.subtitle)}</small>`
        : "";

    return `<button class="key${active ? " is-active" : ""}" ${attrs}
      style="${spanStyle}${tone}">
      <span class="key-icon" aria-hidden="true">${iconMarkup(icon)}</span>
      <span class="key-copy"><strong>${esc(label)}</strong>${subtitle}</span>
      <span class="key-led" aria-hidden="true"></span>
    </button>`;
  }

  function renderDeck() {
    if (!els.deck) return;
    const board = currentBoard();

    if (!board) {
      els.deck.style.gridTemplateColumns = "repeat(4, minmax(0, 1fr))";
      els.deck.innerHTML = `<div class="deck-empty">
        <strong>Waiting for the deck…</strong>
        <span>Connecting to the Liquid Deck server.</span>
      </div>`;
      return;
    }

    const columns = clamp(
      Number(board.columns) || 4,
      MIN_COLUMNS,
      MAX_COLUMNS,
    );
    els.deck.style.gridTemplateColumns = `repeat(${columns}, minmax(0, 1fr))`;

    if (!board.buttons.length) {
      els.deck.innerHTML = `<div class="deck-empty">
        <strong>${esc(board.name)} is empty</strong>
        <span>Add your first action button in the editor.</span>
        <button class="btn btn-ghost" type="button" data-empty-edit>Customize deck</button>
      </div>`;
      return;
    }

    els.deck.innerHTML = board.buttons
      .map((button) => keyMarkup(board, button))
      .join("");
    bindDeckKeys();
  }

  function refreshToggleStates() {
    if (!state.config || !els.deck) return;
    for (const node of $$(".key", els.deck)) {
      const board = findBoard(node.dataset.board);
      const button = board
        ? board.buttons.find((entry) => entry.id === node.dataset.button)
        : null;
      if (board && button) {
        updateKeyState(node, board, button, isActive(board.id, button.id));
      }
    }
  }

  function updateKeyState(node, board, button, active) {
    node.classList.toggle("is-active", Boolean(active));
    node.setAttribute("aria-pressed", String(Boolean(active)));

    const icon =
      active && button.active_icon ? button.active_icon : button.icon || "bolt";
    const label =
      active && button.active_label ? button.active_label : button.label;
    const iconHost = $(".key-icon", node);
    const labelNode = $(".key-copy strong", node);
    if (iconHost) iconHost.innerHTML = iconMarkup(icon);
    if (labelNode) setText(labelNode, label);
  }

  function bindDeckKeys() {
    for (const key of $$(".key", els.deck)) {
      let timer = null;
      let longPressed = false;

      key.addEventListener("pointerdown", (event) => {
        if (event.pointerType === "mouse" && event.button !== 0) return;
        longPressed = false;
        key.classList.add("pressed");
        vibrate(8);
        clearTimeout(timer);
        timer = setTimeout(() => {
          longPressed = true;
          key.classList.remove("pressed");
          vibrate(20);
          openEditor({
            boardId: key.dataset.board,
            buttonId: key.dataset.button,
            tab: "buttons",
          });
        }, LONG_PRESS_MS);
      });

      for (const eventName of ["pointerup", "pointercancel", "pointerleave"]) {
        key.addEventListener(eventName, () => {
          clearTimeout(timer);
          key.classList.remove("pressed");
        });
      }

      key.addEventListener("click", () => {
        if (longPressed) {
          longPressed = false;
          return;
        }
        triggerButton(key);
      });

      key.addEventListener("contextmenu", (event) => {
        event.preventDefault();
        openEditor({
          boardId: key.dataset.board,
          buttonId: key.dataset.button,
          tab: "buttons",
        });
      });
    }
  }

  // -------------------------------------------------------------------------
  // Board navigation
  // -------------------------------------------------------------------------

  function navigateBoard(id, options = {}) {
    const board = findBoard(id);
    if (!board) return;
    const { push = false, persist = true, keepStack = false } = options;

    if (!keepStack) {
      state.boardStack = push && state.activeBoard ? [state.activeBoard] : [];
    }

    state.activeBoard = board.id;
    state.deckState.active_board = board.id;
    renderBoardTabs();
    renderDeck();
    vibrate(6);

    if (persist) postState({ active_board: board.id });
  }

  function goBackBoard() {
    const previous = state.boardStack.pop();
    if (!previous) return;
    navigateBoard(previous, { persist: true, keepStack: true });
  }

  function bindBoards() {
    if (els.boardTabs) {
      els.boardTabs.addEventListener("click", (event) => {
        const back = event.target.closest("[data-board-back]");
        if (back) {
          goBackBoard();
          return;
        }
        const tab = event.target.closest("[data-board]");
        if (tab) navigateBoard(tab.dataset.board);
      });
    }

    if (els.deck) {
      els.deck.addEventListener("click", (event) => {
        if (event.target.closest("[data-empty-edit]")) {
          openEditor({
            boardId: state.activeBoard,
            tab: "buttons",
          });
        }
      });
    }
  }

  async function postState(payload) {
    try {
      const response = await apiRequest("/api/state", {
        method: "POST",
        body: JSON.stringify(payload),
      });
      if (!response.ok) return;
      const data = await response.json().catch(() => null);
      if (data && data.ok && data.state) {
        state.deckState = normalizeDeckState(data.state);
      }
    } catch {
      /* offline: the UI keeps its optimistic state */
    }
  }

  // -------------------------------------------------------------------------
  // Actions
  // -------------------------------------------------------------------------

  function payloadFor(button, active) {
    const amount = clamp(Number(button.amount) || 5, 1, 100);
    switch (button.action) {
      case "discord_mute":
        return { action: "discord_mute" };
      case "discord_deafen":
        return { action: "discord_deafen" };
      case "media":
        return { action: "media", command: button.value || "play_pause" };
      case "volume":
        return {
          action: "volume",
          direction: button.value === "down" ? "down" : "up",
          step: amount,
        };
      case "mute":
        return { action: "mute" };
      case "key":
        return { action: "key", combo: button.value || "" };
      case "script":
        return { action: "script", name: button.value || "" };
      case "launch":
        return { action: "launch", target: button.value || "terminal" };
      case "terminal":
        return { action: "terminal" };
      case "game_mode":
        return { action: "game_mode", enabled: !active };
      case "url":
        return { action: "url", url: button.value || "" };
      case "sound":
        return { action: "sound", name: button.value || "" };
      case "obs":
        return {
          action: "obs",
          op: button.value || "test",
          target: button.arg || "",
        };
      case "audio":
        return {
          action: "audio",
          op: button.value || "output",
          id: button.arg || "",
          value: amount,
        };
      default:
        return null;
    }
  }

  async function triggerButton(node) {
    if (node.classList.contains("busy")) return;

    const board = findBoard(node.dataset.board);
    const button = board
      ? board.buttons.find((entry) => entry.id === node.dataset.button)
      : null;
    if (!board || !button) return;

    if (button.action === "board") {
      if (!button.value) {
        toast("This button has no target board.", "error");
        return;
      }
      navigateBoard(button.value, { push: true });
      return;
    }

    if (button.action === "panel") {
      openPanel(button.value || "editor");
      return;
    }

    if (button.action === "none") {
      toast("This button is not configured yet.", "info");
      return;
    }

    const wasActive = isActive(board.id, button.id);
    const payload = payloadFor(button, wasActive);
    if (!payload) {
      toast("Unsupported action.", "error");
      return;
    }

    node.classList.add("busy");
    try {
      const result = await sendAction(payload);
      handleActionResult(result, board, button, wasActive, node);
    } finally {
      node.classList.remove("busy");
    }
  }

  function openPanel(name) {
    if (name === "audio") {
      openAudio();
    } else if (name === "settings") {
      openEditor({ tab: "integrations" });
    } else {
      openEditor({ tab: "buttons" });
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
      const response = await apiRequest("/api/action", {
        method: "POST",
        body: JSON.stringify(payload),
      });

      if (response.status === 401) {
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

  function actionToggles(button) {
    if (button.toggle) return true;
    if (button.action === "game_mode") return true;
    if (button.action === "discord_mute" || button.action === "discord_deafen") {
      return true;
    }
    if (button.action === "media" && button.value === "play_pause") return true;
    if (
      button.action === "obs" &&
      (button.value === "record_toggle" ||
        button.value === "stream_toggle" ||
        button.value === "mute_toggle")
    ) {
      return true;
    }
    return button.action === "audio" && button.value === "mute";
  }

  function handleActionResult(result, board, button, wasActive, node) {
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

    vibrate(8);

    if (actionToggles(button)) {
      const next = !wasActive;
      setLocalToggle(board.id, button.id, next);
      updateKeyState(node, board, button, next);
      postState({ board: board.id, button: button.id, active: next });
    }

    toast(
      result.message && result.message !== "ok" ? result.message : "Done",
      "success",
    );
  }

  // -------------------------------------------------------------------------
  // Small rendering helpers
  // -------------------------------------------------------------------------

  function esc(value) {
    return String(value == null ? "" : value).replace(/[&<>"']/g, (character) => {
      switch (character) {
        case "&":
          return "&amp;";
        case "<":
          return "&lt;";
        case ">":
          return "&gt;";
        case '"':
          return "&quot;";
        default:
          return "&#39;";
      }
    });
  }

  function resolveColor(color) {
    const value = String(color || "").trim().toLowerCase();
    if (COLORS[value]) return COLORS[value];
    if (/^#[0-9a-f]{6}$/.test(value)) return value;
    return COLORS.cyan;
  }

  function hexToRgb(hex) {
    const value = resolveColor(hex).slice(1);
    return {
      r: parseInt(value.slice(0, 2), 16),
      g: parseInt(value.slice(2, 4), 16),
      b: parseInt(value.slice(4, 6), 16),
    };
  }

  function rgba(hex, alpha) {
    const { r, g, b } = hexToRgb(hex);
    return `rgba(${r}, ${g}, ${b}, ${alpha})`;
  }

  function toneVars(color) {
    const hex = resolveColor(color);
    return `--tone:${hex};--tone-soft:${rgba(hex, 0.18)};--tone-line:${rgba(
      hex,
      0.38,
    )};`;
  }

  function boardTabVars(color) {
    const hex = resolveColor(color);
    return `--tab-accent:${hex};--tab-soft:${rgba(hex, 0.16)};--tab-line:${rgba(
      hex,
      0.36,
    )};`;
  }

  function iconMarkup(name) {
    const value = String(name == null ? "" : name).trim();
    if (!value) return "";
    if (document.getElementById(`icon-${value}`)) {
      return `<svg aria-hidden="true" focusable="false"><use href="#icon-${esc(
        value,
      )}"></use></svg>`;
    }
    return `<span class="key-glyph" aria-hidden="true">${esc(value)}</span>`;
  }

  // -------------------------------------------------------------------------
  // Editor shell
  // -------------------------------------------------------------------------

  function cloneConfig() {
    return JSON.parse(JSON.stringify(state.config));
  }

  function normalizeDraftShape(draft) {
    if (!draft.settings) draft.settings = {};
    if (!draft.settings.obs) {
      draft.settings.obs = { url: "ws://127.0.0.1:4455", password: "" };
    }
    if (!draft.settings.default_board && draft.boards[0]) {
      draft.settings.default_board = draft.boards[0].id;
    }
    for (const board of draft.boards) {
      if (!Array.isArray(board.buttons)) board.buttons = [];
    }
    return draft;
  }

  function editorBoard() {
    const draft = state.editor.draft;
    if (!draft || !Array.isArray(draft.boards)) return null;
    return (
      draft.boards.find((board) => board.id === state.editor.boardId) ||
      draft.boards[0] ||
      null
    );
  }

  function editorButton() {
    const board = editorBoard();
    if (!board || !Array.isArray(board.buttons)) return null;
    return (
      board.buttons.find((button) => button.id === state.editor.buttonId) ||
      board.buttons[0] ||
      null
    );
  }

  function isEditorDirty() {
    const editor = state.editor;
    if (editor.jsonError) return true;
    if (!editor.draft || !state.config) return false;
    return JSON.stringify(editor.draft) !== JSON.stringify(state.config);
  }

  function openEditor(options = {}) {
    if (!state.config) {
      toast("Deck configuration is still loading.", "info");
      return;
    }

    const editor = state.editor;
    let draft;
    try {
      draft = cloneConfig();
    } catch {
      toast("Could not open the editor.", "error");
      return;
    }
    normalizeDraftShape(draft);

    editor.draft = draft;
    editor.boardId =
      options.boardId && draft.boards.some((board) => board.id === options.boardId)
        ? options.boardId
        : state.activeBoard || draft.boards[0].id;

    const board = editorBoard();
    editor.buttonId =
      options.buttonId && board.buttons.some((button) => button.id === options.buttonId)
        ? options.buttonId
        : board.buttons[0]
          ? board.buttons[0].id
          : "";

    editor.jsonText = null;
    editor.jsonError = "";
    editor.saving = false;
    editor.tab = ["boards", "buttons", "integrations", "json"].includes(options.tab)
      ? options.tab
      : options.buttonId
        ? "buttons"
        : "boards";

    openSheet("editor");
    renderEditor();
  }

  function closeEditor(force = false) {
    if (!state.editor.draft) {
      closeSheet("editor");
      return;
    }
    if (!force && isEditorDirty() && !window.confirm("Discard unsaved changes?")) {
      return;
    }
    state.editor.draft = null;
    state.editor.jsonText = null;
    state.editor.jsonError = "";
    closeSheet("editor");
  }

  function switchEditorTab(tab) {
    const editor = state.editor;
    if (tab === editor.tab) return;

    if (editor.tab === "json") {
      if (!applyJsonDraft()) {
        renderEditor();
        toast(editor.jsonError || "Fix the JSON before leaving this tab.", "error");
        return;
      }
      editor.jsonText = null;
    }

    if (tab === "json" && editor.jsonText == null) {
      editor.jsonText = JSON.stringify(editor.draft, null, 2);
      editor.jsonError = "";
    }

    editor.tab = tab;
    renderEditor();
  }

  function renderEditor() {
    if (!els.editorBody) return;
    const editor = state.editor;
    if (!editor.draft) {
      closeSheet("editor");
      return;
    }

    if (els.dirtyBadge) els.dirtyBadge.hidden = !isEditorDirty();
    for (const tab of $$("[data-editor-tab]", els.editorTabs)) {
      tab.classList.toggle("is-active", tab.dataset.editorTab === editor.tab);
      tab.setAttribute("aria-selected", String(tab.dataset.editorTab === editor.tab));
    }

    switch (editor.tab) {
      case "buttons":
        els.editorBody.innerHTML = renderButtonsTab();
        break;
      case "integrations":
        els.editorBody.innerHTML = renderIntegrationsTab();
        break;
      case "json":
        els.editorBody.innerHTML = renderJsonTab();
        break;
      default:
        els.editorBody.innerHTML = renderBoardsTab();
    }
  }

  function renderIconGrid(scope, field, current) {
    const items = ICON_NAMES.map((name) => {
      const active = name === current;
      return `<button class="icon-choice${active ? " is-active" : ""}" type="button"
        data-edit="icon" data-scope="${scope}" data-field="${field}"
        data-icon="${esc(name)}" title="${esc(name)}"
        aria-label="${esc(name)}">${iconMarkup(name)}</button>`;
    }).join("");
    return `<div class="icon-grid">${items}</div>`;
  }

  function renderColorRow(scope, field, current) {
    const value = String(current || "cyan");
    const swatches = COLOR_NAMES.map((name) => {
      const active = name === value;
      return `<button class="swatch${active ? " is-active" : ""}" type="button"
        data-edit="color" data-scope="${scope}" data-field="${field}"
        data-color="${name}" title="${name}" aria-label="${name}"
        style="--swatch:${COLORS[name]};--swatch-soft:${rgba(
          COLORS[name],
          0.45,
        )}"></button>`;
    }).join("");
    return `<div class="color-row">${swatches}
      <input type="color" data-scope="${scope}" data-field="${field}"
        value="${resolveColor(value)}" aria-label="Custom colour">
    </div>`;
  }

  function switchRow(title, hint, scope, field, checked) {
    return `<div class="switch-row">
      <span class="switch-row-copy"><strong>${esc(title)}</strong><small>${esc(
        hint,
      )}</small></span>
      <label class="switch">
        <input type="checkbox" data-scope="${scope}" data-field="${field}" ${
          checked ? "checked" : ""
        }>
        <span class="switch-track" aria-hidden="true"></span>
      </label>
    </div>`;
  }

  // -------------------------------------------------------------------------
  // Editor — boards
  // -------------------------------------------------------------------------

  function renderBoardsTab() {
    const draft = state.editor.draft;
    const board = editorBoard();
    const settings = draft.settings || {};
    const canDelete = draft.boards.length > 1;

    const list = draft.boards
      .map((entry, index) => {
        const active = board && entry.id === board.id;
        return `<div class="editor-item${active ? " is-active" : ""}">
          <button class="editor-item-main" type="button" data-edit="board-select"
            data-id="${esc(entry.id)}">
            <span class="editor-item-icon" style="--item-tone:${resolveColor(
              entry.accent,
            )}">${iconMarkup(entry.icon)}</span>
            <span class="editor-item-copy">
              <strong>${esc(entry.name)}</strong>
              <small>${entry.buttons.length} buttons · ${entry.columns} cols</small>
            </span>
          </button>
          <button class="mini-btn" type="button" data-edit="board-up" data-id="${esc(
            entry.id,
          )}" aria-label="Move board up" ${index === 0 ? "disabled" : ""}>
            ${iconMarkup("arrow-up")}
          </button>
          <button class="mini-btn" type="button" data-edit="board-down" data-id="${esc(
            entry.id,
          )}" aria-label="Move board down" ${
            index === draft.boards.length - 1 ? "disabled" : ""
          }>${iconMarkup("arrow-down")}</button>
          <button class="mini-btn danger" type="button" data-edit="board-delete"
            data-id="${esc(entry.id)}" aria-label="Delete board" ${
              canDelete ? "" : "disabled"
            }>${iconMarkup("trash")}</button>
        </div>`;
      })
      .join("");

    const boardDefaults = draft.boards
      .map(
        (entry) =>
          `<option value="${esc(entry.id)}" ${
            entry.id === settings.default_board ? "selected" : ""
          }>${esc(entry.name)}</option>`,
      )
      .join("");

    const detail = board
      ? `<div class="panel-block">
          <div class="block-head">
            <h3>Board details</h3>
            <small>${esc(board.id)}</small>
          </div>
          <div class="field">
            <label>Name</label>
            <input type="text" maxlength="40" data-scope="board" data-field="name"
              value="${esc(board.name)}" aria-label="Board name">
          </div>
          <div class="field-row">
            <div class="field">
              <label>Columns</label>
              <select data-scope="board" data-field="columns" aria-label="Board columns">
                ${Array.from({ length: MAX_COLUMNS - MIN_COLUMNS + 1 }, (_, index) => {
                  const value = MIN_COLUMNS + index;
                  return `<option value="${value}" ${
                    Number(board.columns) === value ? "selected" : ""
                  }>${value} columns</option>`;
                }).join("")}
              </select>
            </div>
            <div class="field">
              <label>Accent</label>
              <input type="text" maxlength="20" data-scope="board" data-field="accent"
                value="${esc(board.accent)}" aria-label="Board accent">
            </div>
          </div>
          <div class="field">
            <label>Colour</label>
            ${renderColorRow("board", "accent", board.accent)}
          </div>
          <div class="field">
            <label>Icon</label>
            ${renderIconGrid("board", "icon", board.icon)}
          </div>
        </div>`
      : "";

    return `<div class="editor-split">
      <div>
        <div class="panel-block">
          <div class="block-head">
            <h3>Boards</h3>
            <button class="mini-btn" type="button" data-edit="board-add"
              aria-label="Add board" ${draft.boards.length >= MAX_BOARDS ? "disabled" : ""}>
              ${iconMarkup("plus")}
            </button>
          </div>
          <div class="editor-list">${list}</div>
        </div>
      </div>
      <div>
        ${detail}
        <div class="panel-block">
          <div class="block-head"><h3>Deck defaults</h3></div>
          <div class="field">
            <label>Startup board</label>
            <select data-scope="settings" data-field="default_board"
              aria-label="Startup board">${boardDefaults}</select>
          </div>
          ${switchRow(
            "Haptic feedback",
            "Vibration on touch devices",
            "settings",
            "haptics",
            settings.haptics !== false,
          )}
          ${switchRow(
            "Button subtitles",
            "Show the secondary label",
            "settings",
            "show_subtitles",
            settings.show_subtitles !== false,
          )}
        </div>
      </div>
    </div>`;
  }

  // -------------------------------------------------------------------------
  // Editor — buttons
  // -------------------------------------------------------------------------

  function selectMarkup(label, scope, field, entries, current) {
    const options = entries
      .map(([value, text]) => {
        return `<option value="${esc(value)}" ${
          String(value) === String(current) ? "selected" : ""
        }>${esc(text)}</option>`;
      })
      .join("");
    return `<div class="field">
      <label>${esc(label)}</label>
      <select data-scope="${scope}" data-field="${field}"
        aria-label="${esc(label)}">${options}</select>
    </div>`;
  }

  function renderActionFields(button) {
    switch (button.action) {
      case "media":
        return selectMarkup(
          "Command",
          "button",
          "value",
          MEDIA_COMMANDS.map((command) => [
            command,
            command === "play_pause"
              ? "Play / pause"
              : command.charAt(0).toUpperCase() + command.slice(1),
          ]),
          button.value || "play_pause",
        );
      case "volume":
        return `${selectMarkup(
          "Direction",
          "button",
          "value",
          [
            ["up", "Volume up"],
            ["down", "Volume down"],
            ["mute", "Toggle mute"],
          ],
          button.value || "up",
        )}
        <div class="field">
          <label>Step (%)</label>
          <input type="number" min="1" max="100" data-scope="button"
            data-field="amount" value="${clamp(Number(button.amount) || 5, 1, 100)}"
            aria-label="Volume step">
        </div>`;
      case "key":
        return `<div class="field">
          <label>Shortcut</label>
          <input type="text" maxlength="48" data-scope="button" data-field="value"
            value="${esc(button.value)}" placeholder="ctrl+shift+m"
            aria-label="Keyboard shortcut">
        </div>
        <p class="hint">Use + between keys, for example ctrl+shift+m or super+space.</p>`;
      case "script":
        return `<div class="field">
          <label>Script name</label>
          <input type="text" maxlength="128" data-scope="button" data-field="value"
            value="${esc(button.value)}" placeholder="my-script.sh"
            aria-label="Script name">
        </div>
        <p class="hint">${
          state.scriptsEnabled
            ? "The file runs from the scripts directory configured on the server."
            : "Scripts are disabled. Start liquid-deck with --scripts <dir>."
        }</p>`;
      case "launch":
        return selectMarkup(
          "Target",
          "button",
          "value",
          LAUNCH_TARGETS.map((target) => [
            target,
            target.charAt(0).toUpperCase() + target.slice(1),
          ]),
          button.value || "terminal",
        );
      case "url":
        return `<div class="field">
          <label>URL</label>
          <input type="url" maxlength="2048" data-scope="button" data-field="value"
            value="${esc(button.value)}" placeholder="https://example.com"
            aria-label="URL">
        </div>
        <p class="hint">Only http:// and https:// URLs are allowed.</p>`;
      case "sound": {
        const sounds = Array.isArray(state.sounds) ? state.sounds : [];
        const entries = [["", "Choose a sound…"]].concat(
          sounds.map((sound) => [sound.name, sound.name]),
        );
        if (button.value && !sounds.some((sound) => sound.name === button.value)) {
          entries.push([button.value, `${button.value} (missing)`]);
        }
        return `${selectMarkup("Sound", "button", "value", entries, button.value)}
          <p class="hint">${
            state.soundsEnabled
              ? "Copy audio files into the soundboard directory to make them appear here."
              : "The soundboard is disabled. Start liquid-deck with --sounds <dir>."
          }</p>`;
      }
      case "obs":
        return `${selectMarkup(
          "OBS operation",
          "button",
          "value",
          OBS_OPS.map((op) => [
            op,
            op.replace(/_/g, " ").replace(/^./, (letter) => letter.toUpperCase()),
          ]),
          button.value || "scene",
        )}
        <div class="field">
          <label>Scene / source</label>
          <input type="text" maxlength="160" data-scope="button" data-field="arg"
            value="${esc(button.arg)}" placeholder="Main"
            aria-label="OBS scene or source">
        </div>
        <p class="hint">Needed for scene switches and source mute operations. Configure the connection in Integrations.</p>`;
      case "audio":
        return `${selectMarkup(
          "Audio operation",
          "button",
          "value",
          AUDIO_OPS.map((op) => [
            op,
            op.replace(/_/g, " ").replace(/^./, (letter) => letter.toUpperCase()),
          ]),
          button.value || "output",
        )}
        <div class="field">
          <label>Device / app id</label>
          <input type="text" maxlength="160" data-scope="button" data-field="arg"
            value="${esc(button.arg)}" placeholder="Leave empty for default"
            aria-label="Audio device or app id">
        </div>
        <div class="field">
          <label>Level (%)</label>
          <input type="number" min="1" max="100" data-scope="button"
            data-field="amount" value="${clamp(Number(button.amount) || 50, 1, 100)}"
            aria-label="Audio level">
        </div>
        <p class="hint">Copy a device or app id from the mixer, then paste it here.</p>`;
      case "board":
        return selectMarkup(
          "Target board",
          "button",
          "value",
          state.editor.draft.boards.map((board) => [board.id, board.name]),
          button.value,
        );
      case "panel":
        return selectMarkup(
          "Panel",
          "button",
          "value",
          PANELS.map((panel) => [
            panel,
            panel === "audio"
              ? "Audio mixer"
              : panel === "editor"
                ? "Deck editor"
                : "Settings",
          ]),
          button.value || "editor",
        );
      default:
        return '<p class="hint">This action has no extra options.</p>';
    }
  }

  function renderButtonsTab() {
    const board = editorBoard();
    if (!board) return "";

    const button = editorButton();
    const list = board.buttons
      .map((entry) => {
        const active = button && entry.id === button.id;
        return `<div class="editor-item${active ? " is-active" : ""}">
          <button class="editor-item-main" type="button" data-edit="button-select"
            data-id="${esc(entry.id)}">
            <span class="editor-item-icon" style="--item-tone:${resolveColor(
              entry.color,
            )}">${iconMarkup(entry.icon)}</span>
            <span class="editor-item-copy">
              <strong>${esc(entry.label)}</strong>
              <small>${esc(ACTION_LABELS[entry.action] || entry.action)}</small>
            </span>
          </button>
          <button class="mini-btn" type="button" data-edit="button-duplicate"
            data-id="${esc(entry.id)}" aria-label="Duplicate button">
            ${iconMarkup("copy")}
          </button>
          <button class="mini-btn danger" type="button" data-edit="button-delete"
            data-id="${esc(entry.id)}" aria-label="Delete button">
            ${iconMarkup("trash")}
          </button>
        </div>`;
      })
      .join("");

    const detail = button
      ? `<div class="editor-preview">${keyMarkup(board, button, { preview: true })}</div>
        <p class="editor-preview-note">Live preview · edits apply when you save</p>
        <div class="panel-block">
          <div class="block-head">
            <h3>Button</h3>
            <small>${esc(button.id)}</small>
          </div>
          <div class="field">
            <label>Label</label>
            <input type="text" maxlength="48" data-scope="button" data-field="label"
              value="${esc(button.label)}" aria-label="Button label">
          </div>
          <div class="field">
            <label>Subtitle</label>
            <input type="text" maxlength="64" data-scope="button" data-field="subtitle"
              value="${esc(button.subtitle)}" aria-label="Button subtitle">
          </div>
          <div class="field">
            <label>Active label</label>
            <input type="text" maxlength="48" data-scope="button"
              data-field="active_label" value="${esc(button.active_label)}"
              placeholder="Label while active" aria-label="Active label">
          </div>
          <div class="field">
            <label>Colour</label>
            ${renderColorRow("button", "color", button.color)}
          </div>
          <div class="field">
            <label>Icon</label>
            ${renderIconGrid("button", "icon", button.icon)}
          </div>
          <div class="field">
            <label>Active icon</label>
            ${renderIconGrid("button", "active_icon", button.active_icon || button.icon)}
          </div>
          <div class="field">
            <label>Action</label>
            <select data-scope="button" data-field="action" aria-label="Button action">
              ${ACTIONS.map(
                (action) =>
                  `<option value="${action}" ${
                    action === button.action ? "selected" : ""
                  }>${esc(ACTION_LABELS[action] || action)}</option>`,
              ).join("")}
            </select>
          </div>
          ${renderActionFields(button)}
          <div class="field-row">
            <div class="field">
              <label>Width</label>
              <select data-scope="button" data-field="span" aria-label="Button width">
                ${Array.from(
                  { length: Math.min(4, Number(board.columns) || 4) },
                  (_, index) => {
                    const value = index + 1;
                    return `<option value="${value}" ${
                      Number(button.span) === value ? "selected" : ""
                    }>${value} column${value > 1 ? "s" : ""}</option>`;
                  },
                ).join("")}
              </select>
            </div>
            <div class="field">
              <label>State</label>
              <div class="switch-row">
                <span class="switch-row-copy"><strong>Toggle</strong><small>Persist on/off</small></span>
                <label class="switch">
                  <input type="checkbox" data-scope="button" data-field="toggle" ${
                    button.toggle ? "checked" : ""
                  }>
                  <span class="switch-track" aria-hidden="true"></span>
                </label>
              </div>
            </div>
          </div>
        </div>`
      : `<div class="audio-empty"><strong>No buttons yet</strong>Add a button to start building this board.</div>`;

    return `<div class="editor-split">
      <div>
        <div class="panel-block">
          <div class="block-head">
            <h3>${esc(board.name)} buttons</h3>
            <button class="mini-btn" type="button" data-edit="button-add"
              aria-label="Add button" ${
                board.buttons.length >= MAX_BUTTONS ? "disabled" : ""
              }>${iconMarkup("plus")}</button>
          </div>
          <div class="editor-list">${
            list ||
            '<p class="hint">This board is empty. Use + to add a button.</p>'
          }</div>
        </div>
      </div>
      <div>${detail}</div>
    </div>`;
  }

  // -------------------------------------------------------------------------
  // Editor — integrations & JSON
  // -------------------------------------------------------------------------

  function renderIntegrationsTab() {
    const draft = state.editor.draft;
    const obs = draft.settings.obs || { url: "", password: "" };
    const sounds = Array.isArray(state.sounds) ? state.sounds : [];
    const soundRows = sounds.length
      ? sounds
          .map(
            (sound) => `<div class="sound-row">
              <span>${esc(sound.name)}</span>
              <small>${formatBytes(sound.size)}</small>
              <button class="mini-btn" type="button" data-edit="sound-preview"
                data-name="${esc(sound.name)}" aria-label="Preview ${esc(
                  sound.name,
                )}">${iconMarkup("play-circle")}</button>
            </div>`,
          )
          .join("")
      : `<p class="hint">${
          state.soundsEnabled
            ? "No sound files found in the soundboard directory."
            : "Soundboard disabled."
        }</p>`;

    return `<div class="editor-split">
      <div>
        <div class="panel-block">
          <div class="block-head">
            <h3>OBS WebSocket</h3>
            <small>v5 · ws://</small>
          </div>
          <div class="field">
            <label>Server URL</label>
            <input type="text" maxlength="512" data-scope="obs" data-field="url"
              value="${esc(obs.url)}" placeholder="ws://127.0.0.1:4455"
              aria-label="OBS WebSocket URL">
          </div>
          <div class="field">
            <label>Password</label>
            <input type="password" maxlength="256" data-scope="obs"
              data-field="password" value="${esc(obs.password)}"
              autocomplete="off" aria-label="OBS WebSocket password">
          </div>
          <button class="btn btn-ghost" type="button" data-edit="obs-test">
            ${iconMarkup("obs")} Test connection
          </button>
          <div class="integration-status" id="obsStatus"></div>
          <p class="hint">Enable the WebSocket server in OBS under Tools → WebSocket Server Settings. wss:// needs a TLS proxy.</p>
        </div>

        <div class="panel-block">
          <div class="block-head">
            <h3>System integrations</h3>
          </div>
          <div class="settings-row">
            <span class="settings-row-copy"><strong>Audio mixer</strong><small>pactl on Linux · SwitchAudioSource on macOS</small></span>
            <span class="audio-badge">Ready</span>
          </div>
          <div class="settings-row">
            <span class="settings-row-copy"><strong>Scripts</strong><small>${
              state.scriptsEnabled
                ? "A scripts directory is configured"
                : "Disabled · start with --scripts <dir>"
            }</small></span>
            <span class="audio-badge">${state.scriptsEnabled ? "Ready" : "Off"}</span>
          </div>
          <div class="settings-row">
            <span class="settings-row-copy"><strong>URL launcher</strong><small>http:// and https:// only</small></span>
            <span class="audio-badge">Ready</span>
          </div>
        </div>
      </div>
      <div>
        <div class="panel-block">
          <div class="block-head">
            <h3>Soundboard</h3>
            <small>${sounds.length} clips</small>
          </div>
          <div class="sound-list">${soundRows}</div>
          <p class="hint">Sound files live on the server. Use the mixer button beside the board tabs to route audio.</p>
        </div>
      </div>
    </div>`;
  }

  function renderJsonTab() {
    const editor = state.editor;
    if (editor.jsonText == null) editor.jsonText = JSON.stringify(editor.draft, null, 2);
    return `<div class="panel-block">
      <div class="block-head">
        <h3>Raw configuration</h3>
        <small>boards.json</small>
      </div>
      <div class="field">
        <label>JSON</label>
        <textarea id="configJson" spellcheck="false" aria-label="Deck JSON">${esc(
          editor.jsonText,
        )}</textarea>
      </div>
      <p class="hint">The server validates every field before saving. Board ids are normalised automatically.</p>
      <div class="integration-status ${editor.jsonError ? "is-error" : "is-ok"}" id="jsonStatus">${
        editor.jsonError ? esc(editor.jsonError) : "Valid JSON · parsed when you leave this tab"
      }</div>
    </div>`;
  }

  function applyJsonDraft() {
    const editor = state.editor;
    if (editor.jsonText == null) return true;
    try {
      const parsed = JSON.parse(editor.jsonText);
      if (!parsed || typeof parsed !== "object" || !Array.isArray(parsed.boards)) {
        throw new Error("JSON must be an object with a boards array.");
      }
      if (!parsed.boards.length) {
        throw new Error("The deck needs at least one board.");
      }
      if (!parsed.settings) parsed.settings = {};
      if (!parsed.settings.obs) parsed.settings.obs = { url: "", password: "" };
      normalizeDraftShape(parsed);
      editor.draft = parsed;
      editor.jsonError = "";
      const board = editorBoard();
      editor.boardId = board ? board.id : parsed.boards[0].id;
      editor.buttonId = board && board.buttons[0] ? board.buttons[0].id : "";
      return true;
    } catch (error) {
      editor.jsonError = error.message || "Invalid JSON.";
      return false;
    }
  }

  function updateDirtyBadge() {
    if (els.dirtyBadge) els.dirtyBadge.hidden = !isEditorDirty();
  }

  function setDraftValue(scope, field, value) {
    const draft = state.editor.draft;
    if (!draft) return;
    const numeric = field === "columns" || field === "span" || field === "amount";

    if (scope === "board") {
      const board = editorBoard();
      if (!board) return;
      board[field] = numeric ? Number(value) || 0 : value;
      if (field === "columns") {
        const columns = clamp(Number(board.columns) || 4, MIN_COLUMNS, MAX_COLUMNS);
        board.columns = columns;
        for (const button of board.buttons) {
          button.span = clamp(Number(button.span) || 1, 1, Math.min(4, columns));
        }
      }
      return;
    }

    if (scope === "button") {
      const button = editorButton();
      if (!button) return;
      button[field] = numeric ? Number(value) || 0 : value;
      if (field === "span") {
        const board = editorBoard();
        button.span = clamp(
          Number(button.span) || 1,
          1,
          Math.min(4, Number(board.columns) || 4),
        );
      }
      if (field === "amount") button.amount = clamp(Number(button.amount) || 1, 1, 100);
      return;
    }

    if (scope === "settings") {
      draft.settings[field] = value;
      return;
    }

    if (scope === "obs") {
      if (!draft.settings.obs) draft.settings.obs = { url: "", password: "" };
      draft.settings.obs[field] = value;
    }
  }

  function refreshEditorPreview() {
    const board = editorBoard();
    const button = editorButton();
    const host = $(".editor-preview", els.editorBody);
    if (host && board && button) {
      host.innerHTML = keyMarkup(board, button, { preview: true });
    }
  }

  function actionDefaultValue(action) {
    switch (action) {
      case "media":
        return "play_pause";
      case "volume":
        return "up";
      case "launch":
        return "terminal";
      case "obs":
        return "scene";
      case "audio":
        return "output";
      case "panel":
        return "editor";
      case "sound":
        return state.sounds[0] ? state.sounds[0].name : "";
      case "board": {
        const draft = state.editor.draft;
        const other = draft.boards.find((board) => board.id !== state.editor.boardId);
        return other ? other.id : draft.boards[0].id;
      }
      case "url":
        return "https://";
      default:
        return "";
    }
  }

  function updateEditorField(target) {
    const scope = target.dataset.scope;
    const field = target.dataset.field;
    if (!scope || !field) return;

    state.editor.jsonText = null;
    const value =
      target.type === "checkbox" ? target.checked : target.value;

    if (scope === "button" && field === "action") {
      const button = editorButton();
      const next = String(value);
      if (button && button.action !== next) {
        button.action = next;
        button.value = actionDefaultValue(next);
        button.arg = "";
        if (next === "sound") button.amount = 100;
      }
    }

    setDraftValue(scope, field, value);
    updateDirtyBadge();
    refreshEditorPreview();
  }

  // -------------------------------------------------------------------------
  // Editor events
  // -------------------------------------------------------------------------

  function uniqueDraftId(items, base) {
    const root = slug(base || "item") || "item";
    const used = new Set(items.map((item) => item.id));
    let candidate = root;
    let index = 2;
    while (used.has(candidate)) {
      candidate = `${root}-${index}`;
      index += 1;
    }
    return candidate;
  }

  function bindEditor() {
    if (els.editBtn) els.editBtn.addEventListener("click", () => openEditor());
    if (els.editorClose) els.editorClose.addEventListener("click", () => closeEditor());
    if (els.editorCancel) els.editorCancel.addEventListener("click", () => closeEditor());
    if (els.editorSave) els.editorSave.addEventListener("click", saveEditor);
    if (els.editorReset) els.editorReset.addEventListener("click", resetDeck);

    if (els.editorTabs) {
      els.editorTabs.addEventListener("click", (event) => {
        const tab = event.target.closest("[data-editor-tab]");
        if (tab) switchEditorTab(tab.dataset.editorTab);
      });
    }

    if (els.editorBody) {
      els.editorBody.addEventListener("input", (event) => {
        const target = event.target;
        if (target.id === "configJson") {
          state.editor.jsonText = target.value;
          state.editor.jsonError = "";
          const status = $("#jsonStatus", els.editorBody);
          if (status) {
            status.className = "integration-status is-ok";
            status.textContent = "Parsed when you leave this tab";
          }
          updateDirtyBadge();
          return;
        }
        if (!target.dataset || !target.dataset.scope) return;
        updateEditorField(target);
      });

      els.editorBody.addEventListener("change", (event) => {
        const target = event.target;
        if (!target.dataset || !target.dataset.scope) return;
        updateEditorField(target);
        if (target.dataset.scope === "obs") {
          updateDirtyBadge();
          return;
        }
        renderEditor();
      });

      els.editorBody.addEventListener("click", handleEditorClick);
    }
  }

  async function handleEditorClick(event) {
    const target = event.target.closest("[data-edit]");
    if (!target) return;
    const edit = target.dataset.edit;
    const draft = state.editor.draft;
    if (!draft) return;

    if (edit === "board-select") {
      state.editor.boardId = target.dataset.id;
      const board = editorBoard();
      state.editor.buttonId = board && board.buttons[0] ? board.buttons[0].id : "";
      renderEditor();
      return;
    }

    if (edit === "board-add") {
      if (draft.boards.length >= MAX_BOARDS) {
        toast("The deck is limited to 16 boards.", "error");
        return;
      }
      const id = uniqueDraftId(draft.boards, "board");
      draft.boards.push({
        id,
        name: "New Board",
        icon: "grid",
        accent: COLORS[COLOR_NAMES[draft.boards.length % COLOR_NAMES.length]],
        columns: 4,
        buttons: [],
      });
      state.editor.boardId = id;
      state.editor.buttonId = "";
      renderEditor();
      return;
    }

    if (edit === "board-delete") {
      if (draft.boards.length <= 1) {
        toast("The deck needs at least one board.", "error");
        return;
      }
      const id = target.dataset.id;
      const board = draft.boards.find((entry) => entry.id === id);
      if (!board || !window.confirm(`Delete “${board.name}” and its buttons?`)) return;
      draft.boards = draft.boards.filter((entry) => entry.id !== id);
      for (const entry of draft.boards) {
        for (const button of entry.buttons) {
          if (button.action === "board" && button.value === id) {
            button.action = "none";
            button.value = "";
            button.arg = "";
          }
        }
      }
      if (draft.settings.default_board === id) {
        draft.settings.default_board = draft.boards[0].id;
      }
      if (state.editor.boardId === id) {
        state.editor.boardId = draft.boards[0].id;
        state.editor.buttonId = draft.boards[0].buttons[0]
          ? draft.boards[0].buttons[0].id
          : "";
      }
      renderEditor();
      return;
    }

    if (edit === "board-up" || edit === "board-down") {
      const index = draft.boards.findIndex((entry) => entry.id === target.dataset.id);
      const swap = edit === "board-up" ? index - 1 : index + 1;
      if (index < 0 || swap < 0 || swap >= draft.boards.length) return;
      [draft.boards[index], draft.boards[swap]] = [
        draft.boards[swap],
        draft.boards[index],
      ];
      renderEditor();
      return;
    }

    if (edit === "button-select") {
      state.editor.buttonId = target.dataset.id;
      renderEditor();
      return;
    }

    if (edit === "button-add") {
      const board = editorBoard();
      if (!board) return;
      if (board.buttons.length >= MAX_BUTTONS) {
        toast("A board is limited to 32 buttons.", "error");
        return;
      }
      const id = uniqueDraftId(board.buttons, "button");
      board.buttons.push({
        id,
        label: "New Button",
        subtitle: "",
        icon: "bolt",
        active_label: "",
        active_icon: "",
        color: "cyan",
        action: "none",
        value: "",
        arg: "",
        amount: 5,
        toggle: false,
        span: 1,
      });
      state.editor.buttonId = id;
      renderEditor();
      return;
    }

    if (edit === "button-delete") {
      const board = editorBoard();
      if (!board) return;
      const index = board.buttons.findIndex((entry) => entry.id === target.dataset.id);
      if (index < 0) return;
      const button = board.buttons[index];
      if (!window.confirm(`Delete “${button.label}”?`)) return;
      board.buttons.splice(index, 1);
      const next = board.buttons[Math.min(index, board.buttons.length - 1)];
      state.editor.buttonId = next ? next.id : "";
      renderEditor();
      return;
    }

    if (edit === "button-duplicate") {
      const board = editorBoard();
      if (!board) return;
      const index = board.buttons.findIndex((entry) => entry.id === target.dataset.id);
      if (index < 0 || board.buttons.length >= MAX_BUTTONS) return;
      const source = board.buttons[index];
      const copy = JSON.parse(JSON.stringify(source));
      copy.id = uniqueDraftId(board.buttons, `${source.id}-copy`);
      copy.label = `${source.label} copy`;
      board.buttons.splice(index + 1, 0, copy);
      state.editor.buttonId = copy.id;
      renderEditor();
      return;
    }

    if (edit === "icon") {
      setDraftValue(target.dataset.scope, target.dataset.field, target.dataset.icon);
      updateDirtyBadge();
      renderEditor();
      return;
    }

    if (edit === "color") {
      setDraftValue(target.dataset.scope, target.dataset.field, target.dataset.color);
      updateDirtyBadge();
      renderEditor();
      return;
    }

    if (edit === "sound-preview") {
      previewSound(target.dataset.name);
      return;
    }

    if (edit === "obs-test") {
      const status = $("#obsStatus", els.editorBody);
      const obs = draft.settings.obs || {};
      if (status) {
        status.className = "integration-status";
        status.textContent = "Testing connection…";
      }
      try {
        const response = await apiRequest("/api/obs/test", {
          method: "POST",
          body: JSON.stringify({ url: obs.url || "", password: obs.password || "" }),
        });
        const data = await response.json().catch(() => null);
        if (status) {
          status.className = `integration-status ${
            data && data.ok ? "is-ok" : "is-error"
          }`;
          status.textContent =
            (data && data.message) || `Test failed (HTTP ${response.status}).`;
        }
        vibrate(data && data.ok ? 8 : [18, 40, 18]);
      } catch {
        if (status) {
          status.className = "integration-status is-error";
          status.textContent = "Could not reach the server.";
        }
      }
    }
  }

  async function saveEditor() {
    const editor = state.editor;
    if (!editor.draft || editor.saving) return;

    if (editor.jsonText != null && !applyJsonDraft()) {
      editor.tab = "json";
      renderEditor();
      toast(editor.jsonError || "Fix the JSON first.", "error");
      return;
    }

    editor.saving = true;
    if (els.editorSave) els.editorSave.disabled = true;
    try {
      const response = await apiRequest("/api/config", {
        method: "PUT",
        body: JSON.stringify(editor.draft),
      });
      const data = await response.json().catch(() => null);

      if (!response.ok || !data || !data.ok) {
        const message =
          (data && data.message) || `Save failed (HTTP ${response.status}).`;
        toast(message, "error");
        if (editor.tab === "json") {
          editor.jsonError = message;
          renderEditor();
        }
        vibrate([18, 40, 18]);
        return;
      }

      if (Array.isArray(data.sounds)) state.sounds = data.sounds;
      editor.draft = null;
      editor.jsonText = null;
      editor.jsonError = "";
      closeSheet("editor");
      applyDeckPayload(data.config, data.state);
      if (data.warning) toast(data.warning, "error");
      else toast("Deck saved.", "success");
      vibrate(8);
    } catch {
      toast("Could not save the deck.", "error");
    } finally {
      editor.saving = false;
      if (els.editorSave) els.editorSave.disabled = false;
    }
  }

  async function resetDeck() {
    if (
      !window.confirm(
        "Reset every board, button and integration back to the built-in deck?",
      )
    ) {
      return;
    }
    try {
      const response = await apiRequest("/api/config/reset", { method: "POST" });
      const data = await response.json().catch(() => null);
      if (!response.ok || !data || !data.ok) {
        toast((data && data.message) || "Could not reset the deck.", "error");
        return;
      }
      if (Array.isArray(data.sounds)) state.sounds = data.sounds;
      state.editor.draft = null;
      state.editor.jsonText = null;
      state.editor.jsonError = "";
      closeSheet("editor");
      applyDeckPayload(data.config, data.state);
      toast("Deck reset to defaults.", "success");
    } catch {
      toast("Could not reset the deck.", "error");
    }
  }

  function previewSound(name) {
    if (!name) return;
    if (state.previewAudio) {
      try {
        state.previewAudio.pause();
      } catch {
        /* already stopped */
      }
    }
    const params = new URLSearchParams({ name });
    if (state.token) params.set("token", state.token);
    const audio = new Audio(`/api/sound?${params}`);
    state.previewAudio = audio;
    audio.play().catch(() => toast("Could not preview that sound.", "error"));
  }

  // -------------------------------------------------------------------------
  // Audio mixer
  // -------------------------------------------------------------------------

  function bindAudio() {
    if (els.audioBtn) els.audioBtn.addEventListener("click", openAudio);
    if (els.audioClose) els.audioClose.addEventListener("click", () => closeSheet("audio"));
    if (els.audioRefresh) {
      els.audioRefresh.addEventListener("click", () => loadAudio(true));
    }

    if (els.audioBody) {
      els.audioBody.addEventListener("input", (event) => {
        const range = event.target.closest("[data-audio-volume]");
        if (!range) return;
        const percent = range.parentElement
          ? $(".audio-percent", range.parentElement)
          : null;
        if (percent) setText(percent, `${range.value}%`);
      });

      els.audioBody.addEventListener("change", (event) => {
        const range = event.target.closest("[data-audio-volume]");
        if (!range) return;
        sendAudio(range.dataset.op, range.dataset.id, Number(range.value), {
          quiet: true,
        });
      });

      els.audioBody.addEventListener("click", async (event) => {
        const action = event.target.closest("[data-audio]");
        if (!action) return;
        const result = await sendAudio(
          action.dataset.audio,
          action.dataset.id || "",
          undefined,
        );
        if (result && result.ok) loadAudio(false);
      });
    }
  }

  async function openAudio() {
    openSheet("audio");
    if (els.audioBody) {
      els.audioBody.innerHTML =
        '<div class="audio-empty"><strong>Reading audio devices…</strong>This can take a second on PipeWire.</div>';
    }
    await loadAudio(true);
  }

  async function loadAudio(showSpinner) {
    if (!els.audioBody) return;
    if (showSpinner && els.audioBody.children.length === 0) {
      els.audioBody.innerHTML =
        '<div class="audio-empty"><strong>Reading audio devices…</strong></div>';
    }
    try {
      const response = await apiRequest("/api/audio");
      if (!response.ok) {
        els.audioBody.innerHTML =
          '<div class="audio-empty"><strong>Mixer unavailable</strong>Could not read the audio state.</div>';
        return;
      }
      const snapshot = await response.json();
      renderAudio(snapshot);
    } catch {
      els.audioBody.innerHTML =
        '<div class="audio-empty"><strong>Mixer offline</strong>The server is not reachable.</div>';
    }
  }

  function renderAudio(snapshot) {
    if (!els.audioBody) return;
    if (!snapshot || !snapshot.supported) {
      els.audioBody.innerHTML = `<div class="audio-empty">
        <strong>Mixer unavailable</strong>
        ${esc(
          (snapshot && snapshot.message) ||
            "Audio control is not available on this platform.",
        )}
      </div>`;
      return;
    }

    const devices = Array.isArray(snapshot.devices) ? snapshot.devices : [];
    const apps = Array.isArray(snapshot.apps) ? snapshot.apps : [];
    const backend = snapshot.backend || "system audio";

    const deviceMarkup = devices.length
      ? devices
          .map((device) => {
            const muted = device.muted ? " · muted" : "";
            const useButton = device.is_default
              ? '<span class="audio-badge">Default</span>'
              : `<button class="audio-use" type="button" data-audio="output"
                  data-id="${esc(device.id)}">Use</button>`;
            return `<div class="audio-device${device.is_default ? " is-default" : ""}">
              <div class="audio-main">
                <span class="audio-main-icon" aria-hidden="true">${iconMarkup("speaker")}</span>
                <span class="audio-main-copy">
                  <strong>${esc(device.description)}</strong>
                  <small>${esc(device.id)}${muted}</small>
                </span>
                ${useButton}
                <button class="mini-btn" type="button" data-audio="mute"
                  data-id="${esc(device.id)}" aria-label="Mute output">
                  ${iconMarkup("volume-x")}
                </button>
              </div>
              <div class="audio-level">
                <input type="range" min="0" max="100" value="${clamp(
                  Number(device.volume) || 0,
                  0,
                  100,
                )}" data-audio-volume data-op="volume" data-id="${esc(
                  device.id,
                )}" aria-label="Output volume">
                <span class="audio-percent">${clamp(
                  Number(device.volume) || 0,
                  0,
                  100,
                )}%</span>
              </div>
            </div>`;
          })
          .join("")
      : '<div class="audio-empty">No output devices were reported.</div>';

    const appMarkup = apps.length
      ? apps
          .map((app) => {
            const muted = app.muted ? " · muted" : "";
            return `<div class="audio-device">
              <div class="audio-main">
                <span class="audio-main-icon" aria-hidden="true">${iconMarkup("music")}</span>
                <span class="audio-main-copy">
                  <strong>${esc(app.app || "Application")}</strong>
                  <small>${esc(app.title || "Playing audio")}${muted}</small>
                </span>
                <button class="mini-btn" type="button" data-audio="app_mute"
                  data-id="${esc(app.id)}" aria-label="Mute application">
                  ${iconMarkup("volume-x")}
                </button>
              </div>
              <div class="audio-level">
                <input type="range" min="0" max="100" value="${clamp(
                  Number(app.volume) || 0,
                  0,
                  100,
                )}" data-audio-volume data-op="app_volume" data-id="${esc(
                  app.id,
                )}" aria-label="Application volume">
                <span class="audio-percent">${clamp(
                  Number(app.volume) || 0,
                  0,
                  100,
                )}%</span>
              </div>
            </div>`;
          })
          .join("")
      : '<div class="audio-empty">No applications are playing audio right now.</div>';

    els.audioBody.innerHTML = `<div class="audio-meta">
        <span>${esc(backend)}</span>
        <span>${devices.length} output${devices.length === 1 ? "" : "s"} · ${
          apps.length
        } app${apps.length === 1 ? "" : "s"}</span>
      </div>
      <div class="audio-section">
        <h3>Output devices</h3>
        ${deviceMarkup}
      </div>
      <div class="audio-section">
        <h3>Applications</h3>
        ${appMarkup}
      </div>`;
  }

  async function sendAudio(op, id, value, options = {}) {
    try {
      const body = { op, id: id || "" };
      if (Number.isFinite(value)) body.value = clamp(Number(value), 0, 100);
      const response = await apiRequest("/api/audio", {
        method: "POST",
        body: JSON.stringify(body),
      });
      const data = await response.json().catch(() => null);
      if (!options.quiet) {
        toast(
          (data && data.message) || (data && data.ok ? "Audio updated." : "Audio failed."),
          data && data.ok ? "success" : "error",
        );
      }
      return data;
    } catch {
      if (!options.quiet) toast("Could not reach the audio mixer.", "error");
      return null;
    }
  }

  // -------------------------------------------------------------------------
  // Sheets
  // -------------------------------------------------------------------------

  function openSheet(name) {
    const sheet = name === "editor" ? els.editorSheet : els.audioSheet;
    if (!sheet) return;
    const other = name === "editor" ? els.audioSheet : els.editorSheet;
    if (other) other.hidden = true;
    sheet.hidden = false;
    state.sheet = name;
    document.body.classList.add("is-sheet-open");
  }

  function closeSheet(name) {
    const sheet = name === "editor" ? els.editorSheet : els.audioSheet;
    if (sheet) sheet.hidden = true;
    if (state.sheet === name) state.sheet = null;
    const anyOpen = [els.editorSheet, els.audioSheet].some(
      (entry) => entry && !entry.hidden,
    );
    if (!anyOpen) document.body.classList.remove("is-sheet-open");
  }

  function bindSheets() {
    document.addEventListener("click", (event) => {
      const target = event.target.closest("[data-close]");
      if (!target) return;
      if (target.dataset.close === "editor") closeEditor();
      else closeSheet(target.dataset.close);
    });

    document.addEventListener("keydown", (event) => {
      if (event.key !== "Escape" || !state.sheet) return;
      if (state.sheet === "editor") closeEditor();
      else closeSheet(state.sheet);
    });
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
      await loadDeck();
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
    closeSheet("audio");
    closeSheet("editor");
    state.editor.draft = null;
    state.sheet = null;

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
    if (state.config && state.config.settings && state.config.settings.haptics === false) {
      return;
    }
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

  function slug(input) {
    let out = "";
    let dash = true;
    for (const character of String(input || "").toLowerCase()) {
      if (/[a-z0-9]/.test(character)) {
        out += character;
        dash = false;
      } else if (!dash) {
        out += "-";
        dash = true;
      }
      if (out.length >= 32) break;
    }
    return out.replace(/^-+|-+$/g, "");
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

    const digits = index === 0 ? 0 : scaled >= 100 ? 0 : 1;
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

    const digits = index === 0 ? 0 : scaled >= 100 ? 0 : 1;
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
