/* Phone-local rolling telemetry. No timers, network calls or server-side logs. */
(function (root) {
  "use strict";
  const DAY = 24 * 60 * 60 * 1000;
  const MINUTE = 60000;
  const FIELDS = ["cpu", "temp", "ram_pct", "net_rx", "net_tx", "ping", "jitter", "gpu_usage", "gpu_temp", "gpu_memory_pct", "gpu_power_watts", "gpu_clock_mhz"];
  const finite = (value) => typeof value === "number" && Number.isFinite(value);
  function pingDelta(previous, previousTs, current, currentTs) {
    if (!finite(previous) || !finite(current) || previousTs <= 0 || currentTs <= previousTs || currentTs - previousTs > 15000) return null;
    return Math.abs(current - previous);
  }

  function summarize(record, samples) {
    const row = record || { lastTs: 0, lastPingTs: 0, samples: 0, values: {}, events: [] };
    for (const sample of samples) {
      if (!finite(sample.ts) || sample.ts <= row.lastTs) continue;
      const freshPing = finite(sample.ping_ts) && sample.ping_ts > row.lastPingTs;
      row.lastTs = sample.ts;
      row.samples += 1;
      for (const field of FIELDS) {
        if ((field === "ping" || field === "jitter") && !freshPing) continue;
        const value = sample[field];
        if (!finite(value)) continue; // null sensors are not zero-degree readings.
        const stat = row.values[field] || { n: 0, sum: 0, min: value, max: value };
        stat.n += 1;
        stat.sum += value;
        stat.min = Math.min(stat.min, value);
        stat.max = Math.max(stat.max, value);
        row.values[field] = stat;
      }
      if (freshPing) {
        row.lastPingTs = sample.ping_ts;
        if (sample.alert && row.events.length < 4) {
          row.events.push({ t: sample.received, delta: sample.jitter, ping: sample.ping });
        }
      }
    }
    return row;
  }

  class MinuteLog {
    constructor(onStatus) {
      this.onStatus = onStatus || (() => {});
      this.pending = [];
      this.source = "";
      this.minute = 0;
      this.queue = Promise.resolve();
      this.failed = false;
      this.db = new Promise((resolve, reject) => {
        if (!root.indexedDB) { reject(new Error("IndexedDB desteklenmiyor")); return; }
        const request = root.indexedDB.open("liquid-deck-telemetry", 1);
        request.onupgradeneeded = () => {
          const store = request.result.createObjectStore("minutes", { keyPath: ["source", "minute"] });
          store.createIndex("minute", "minute");
        };
        request.onsuccess = () => {
          const db = request.result;
          db.onversionchange = () => { db.close(); this.fail(new Error("Kayıt deposu değişti; sayfayı yenileyin")); };
          resolve(db);
          this.onStatus("Yerel kayıt hazır");
        };
        request.onerror = () => reject(request.error || new Error("Yerel depo açılamadı"));
        request.onblocked = () => this.onStatus("Depo kilitli; diğer panel sekmelerini kapatın");
      });
      // Keep failures visible, but never interrupt live telemetry.
      this.db.catch((error) => this.fail(error));
    }

    fail(error) {
      this.failed = true;
      this.pending = [];
      this.onStatus(`Kayıt yapılamıyor: ${error.message || error}`);
    }

    ingest(metrics, jitter, alert, freshPing) {
      if (this.failed || !finite(metrics.ts)) return;
      const source = String(metrics.host || "unknown");
      const received = Date.now();
      const minute = Math.floor(received / MINUTE) * MINUTE;
      if (this.pending.length && (this.minute !== minute || this.source !== source)) this.flush();
      this.source = source;
      this.minute = minute;
      if (this.pending.some((sample) => sample.ts === metrics.ts)) return;
      const sample = { ts: metrics.ts, received, ping_ts: freshPing ? metrics.ping_ts : 0, jitter: freshPing ? jitter : null, alert: Boolean(alert) };
      for (const field of FIELDS) if (field !== "jitter") sample[field] = metrics[field];
      const gpu = metrics.gpu || {};
      sample.gpu_usage = gpu.usage;
      sample.gpu_temp = gpu.temp;
      sample.gpu_memory_pct = gpu.memory_pct;
      sample.gpu_power_watts = gpu.power_watts;
      sample.gpu_clock_mhz = gpu.clock_mhz;
      this.pending.push(sample);
      if (this.pending.length >= 256) this.flush();
    }

    flush() {
      if (!this.pending.length || this.failed) return this.queue;
      const samples = this.pending;
      const source = this.source;
      const minute = this.minute;
      this.pending = [];
      this.queue = this.queue.then(async () => {
        const db = await this.db;
        await new Promise((resolve, reject) => {
          const tx = db.transaction("minutes", "readwrite");
          const store = tx.objectStore("minutes");
          const request = store.get([source, minute]);
          request.onsuccess = () => {
            const row = summarize(request.result, samples);
            store.put(Object.assign(row, { source, minute }));
          };
          // Indexed expiry: visit old rows only, not the entire 24-hour history.
          const expired = store.index("minute").openCursor(IDBKeyRange.upperBound(Date.now() - DAY, true));
          expired.onsuccess = () => { const cursor = expired.result; if (cursor) { cursor.delete(); cursor.continue(); } };
          tx.oncomplete = resolve;
          tx.onabort = () => reject(tx.error || new Error("Yerel kayıt yazılamadı"));
          tx.onerror = () => {}; // onabort reports the failure once.
        });
        this.onStatus("24 saat · Bu cihazda kayıt");
      }).catch((error) => this.fail(error));
      return this.queue;
    }

    async read(source) {
      await this.flush();
      if (this.failed) throw new Error("Yerel kayıt kullanılamıyor");
      const db = await this.db;
      return new Promise((resolve, reject) => {
        const tx = db.transaction("minutes", "readonly");
        const range = IDBKeyRange.bound([source, Date.now() - DAY], [source, Number.MAX_SAFE_INTEGER]);
        const request = tx.objectStore("minutes").getAll(range);
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
    }

    async clear(source) {
      await this.flush();
      const db = await this.db;
      await new Promise((resolve, reject) => {
        const tx = db.transaction("minutes", "readwrite");
        const range = IDBKeyRange.bound([source, 0], [source, Number.MAX_SAFE_INTEGER]);
        const request = tx.objectStore("minutes").openCursor(range);
        request.onsuccess = () => { const cursor = request.result; if (cursor) { cursor.delete(); cursor.continue(); } };
        tx.oncomplete = resolve;
        tx.onabort = () => reject(tx.error);
      });
    }
  }
  root.LiquidHistory = { MinuteLog, summarize, pingDelta, DAY, MINUTE, FIELDS };
})(typeof window === "undefined" ? globalThis : window);
