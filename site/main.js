// Kraken Control site: Rapid Trigger demo and latest-release download link.
"use strict";

const TRAVEL_MM = 4.0;

function rapidTriggerDemo() {
  const key = document.getElementById("key");
  if (!key) return;
  const well = key.closest(".key-well");
  const fill = document.getElementById("fill");
  const markAp = document.getElementById("markAp");
  const markRt = document.getElementById("markRt");
  const markRtLabel = document.getElementById("markRtLabel");
  const depthOut = document.getElementById("depth");
  const stateOut = document.getElementById("state");
  const countOut = document.getElementById("count");
  const ap = document.getElementById("ap");
  const apOut = document.getElementById("apOut");
  const rt = document.getElementById("rt");
  const sens = document.getElementById("sens");
  const sensOut = document.getElementById("sensOut");
  const track = fill.parentElement;
  const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  let depth = 0;          // mm pressed
  let pressed = false;    // what the computer sees
  let extreme = 0;        // deepest point while pressed, shallowest while released (RT)
  let presses = 0;
  let anim = 0;

  const pct = (mm) => (mm / TRAVEL_MM) * 100;

  function update(d) {
    depth = Math.min(TRAVEL_MM, Math.max(0, d));
    const point = Number(ap.value);
    const s = Number(sens.value);
    const was = pressed;

    if (!rt.checked) {
      pressed = depth >= point;
    } else if (depth < point) {
      pressed = false;               // above the actuation point the key is always up
      extreme = depth;
    } else if (pressed) {
      if (depth <= extreme - s) { pressed = false; extreme = depth; }
      else extreme = Math.max(extreme, depth);
    } else {
      // first crossing of the actuation point, or a push of `s` from the highest point
      if (extreme < point || depth >= extreme + s) { pressed = true; extreme = depth; }
      else extreme = Math.min(extreme, depth);
    }
    if (pressed && !was) presses += 1;
    render(point, s);
  }

  function render(point, s) {
    key.style.setProperty("--depth", (depth / TRAVEL_MM).toFixed(4));
    well.classList.toggle("on", pressed);
    fill.style.height = pct(depth) + "%";
    fill.classList.toggle("on", pressed);
    markAp.style.top = pct(point) + "%";

    const showRt = rt.checked && depth >= point;
    markRt.classList.toggle("show", showRt);
    if (showRt) {
      const at = pressed ? Math.max(point, extreme - s) : Math.min(TRAVEL_MM, extreme + s);
      markRt.style.top = pct(at) + "%";
      markRtLabel.textContent = pressed ? "Release" : "Press again";
    }

    depthOut.textContent = depth.toFixed(2);
    stateOut.textContent = pressed ? "Pressed" : "Released";
    stateOut.classList.toggle("on", pressed);
    countOut.textContent = String(presses);
    key.setAttribute("aria-valuenow", depth.toFixed(2));
    key.setAttribute("aria-valuetext", depth.toFixed(2) + " millimetres, " + (pressed ? "pressed" : "released"));
  }

  function animateTo(target, mmPerSecond) {
    cancelAnimationFrame(anim);
    if (reduceMotion) { update(target); return; }
    let last = performance.now();
    const step = (now) => {
      const dt = (now - last) / 1000;
      last = now;
      const dir = Math.sign(target - depth);
      const next = depth + dir * mmPerSecond * dt;
      if ((dir > 0 && next >= target) || (dir < 0 && next <= target) || dir === 0) { update(target); return; }
      update(next);
      anim = requestAnimationFrame(step);
    };
    anim = requestAnimationFrame(step);
  }

  // Drag: the full gauge height equals the full key travel.
  let drag = null;
  key.addEventListener("pointerdown", (e) => {
    cancelAnimationFrame(anim);
    key.setPointerCapture(e.pointerId);
    drag = { y: e.clientY, start: depth, perMm: track.clientHeight / TRAVEL_MM };
  });
  key.addEventListener("pointermove", (e) => {
    if (!drag) return;
    update(drag.start + (e.clientY - drag.y) / drag.perMm);
  });
  const endDrag = () => {
    if (!drag) return;
    drag = null;
    animateTo(0, 40);   // a released key springs back
  };
  key.addEventListener("pointerup", endDrag);
  key.addEventListener("pointercancel", endDrag);

  // Keyboard: arrows move in 0.1 mm steps (Shift: 0.5 mm), Space/Enter holds the key down.
  let holding = false;
  key.addEventListener("keydown", (e) => {
    const big = e.shiftKey ? 0.5 : 0.1;
    switch (e.key) {
      case "ArrowDown": case "ArrowRight": cancelAnimationFrame(anim); update(depth + big); break;
      case "ArrowUp": case "ArrowLeft": cancelAnimationFrame(anim); update(depth - big); break;
      case "Home": cancelAnimationFrame(anim); update(0); break;
      case "End": cancelAnimationFrame(anim); update(TRAVEL_MM); break;
      case " ": case "Enter":
        if (!holding) { holding = true; animateTo(TRAVEL_MM, 16); }
        break;
      default: return;
    }
    e.preventDefault();
  });
  key.addEventListener("keyup", (e) => {
    if ((e.key === " " || e.key === "Enter") && holding) {
      holding = false;
      animateTo(0, 40);
      e.preventDefault();
    }
  });

  const refreshLabels = () => {
    apOut.textContent = Number(ap.value).toFixed(2) + " mm";
    sensOut.textContent = Number(sens.value).toFixed(2) + " mm";
    sens.disabled = !rt.checked;
    extreme = depth;
    update(depth);
  };
  [ap, sens, rt].forEach((el) => el.addEventListener("input", refreshLabels));
  refreshLabels();
}

// Point every Download link at the newest release's zip and show its version and size.
async function latestRelease() {
  const links = document.querySelectorAll(".js-download");
  const meta = document.querySelector(".js-release-meta");
  try {
    const res = await fetch("https://api.github.com/repos/ripDZL/Kraken-Control/releases/latest", {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!res.ok) return;
    const rel = await res.json();
    const zip = (rel.assets || []).find((a) => a.name.endsWith(".zip"));
    if (!zip) return;
    links.forEach((a) => { a.href = zip.browser_download_url; });
    const mb = (zip.size / (1024 * 1024)).toFixed(1);
    const version = String(rel.tag_name || "").replace(/^v/, "");
    if (meta) meta.textContent = "Version " + version + ", portable zip, " + mb + " MB. Windows 10 and 11.";
  } catch (_) {
    // Offline or rate-limited: links keep pointing at the releases page.
  }
}

// Live test: show exactly when the keyboard reports presses and releases.
// A web page can't read key depth, only these moments.
function liveKeyboard(isActive) {
  const canvas = document.getElementById("liveCanvas");
  if (!canvas) return { start() {}, stop() {} };
  const ctx = canvas.getContext("2d");
  const empty = document.getElementById("liveEmpty");
  const out = {
    count: document.getElementById("lvCount"),
    rate: document.getElementById("lvRate"),
    gap: document.getElementById("lvGap"),
    hold: document.getElementById("lvHold"),
  };
  const WINDOW_MS = 3000;
  const MAX_LANES = 4;
  const css = getComputedStyle(document.documentElement);
  const col = {
    teal: css.getPropertyValue("--teal").trim() || "#3CC8B4",
    line: css.getPropertyValue("--g-line").trim() || "#36383D",
    text: css.getPropertyValue("--g-body").trim() || "#DCDEE1",
    muted: css.getPropertyValue("--g-muted").trim() || "#ABAFB6",
  };
  const lanes = [];            // { code, label, bars: [{down, up}] , lastUp }
  const downs = [];            // press times, for the per-second rate
  let presses = 0, bestGap = Infinity, bestHold = Infinity, raf = 0, running = false;

  const label = (e) => {
    const c = e.code || "";
    if (c.startsWith("Key")) return c.slice(3);
    if (c.startsWith("Digit")) return c.slice(5);
    if (c.startsWith("Numpad")) return "Num " + c.slice(6);
    const names = { Space: "Space", ShiftLeft: "L Shift", ShiftRight: "R Shift", ControlLeft: "L Ctrl", ControlRight: "R Ctrl",
      AltLeft: "L Alt", AltRight: "R Alt", ArrowUp: "Up", ArrowDown: "Down", ArrowLeft: "Left", ArrowRight: "Right",
      Enter: "Enter", Backspace: "Backspace", Tab: "Tab", Escape: "Esc", CapsLock: "Caps" };
    return names[c] || (e.key && e.key.length === 1 ? e.key.toUpperCase() : c || e.key);
  };
  const lane = (e) => {
    let l = lanes.find((x) => x.code === e.code);
    if (!l) {
      l = { code: e.code, label: label(e), bars: [], lastUp: null };
      lanes.unshift(l);
      if (lanes.length > MAX_LANES) lanes.pop();
    } else {
      lanes.splice(lanes.indexOf(l), 1);
      lanes.unshift(l);
    }
    return l;
  };
  const ms = (v) => (v === Infinity ? "–" : Math.round(v) + " ms");

  function onDown(e) {
    if (!isActive() || e.repeat || e.metaKey) return;
    if (e.target.closest && e.target.closest("input, textarea, select")) return;
    if ([" ", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Backspace"].includes(e.key) && !e.ctrlKey && !e.altKey) e.preventDefault();
    const t = e.timeStamp;
    const l = lane(e);
    if (l.bars.length && l.bars[l.bars.length - 1].up === null) return;
    if (l.lastUp !== null) bestGap = Math.min(bestGap, t - l.lastUp);
    l.bars.push({ down: t, up: null });
    presses += 1;
    downs.push(t);
    empty.hidden = true;
    stats();
    start();
  }
  function onUp(e) {
    if (!isActive()) return;
    const l = lanes.find((x) => x.code === e.code);
    if (!l || !l.bars.length) return;
    const b = l.bars[l.bars.length - 1];
    if (b.up !== null) return;
    b.up = e.timeStamp;
    l.lastUp = b.up;
    bestHold = Math.min(bestHold, b.up - b.down);
    stats();
  }
  function releaseAll() {
    const t = performance.now();
    lanes.forEach((l) => { const b = l.bars[l.bars.length - 1]; if (b && b.up === null) { b.up = t; l.lastUp = null; } });
  }
  function stats() {
    const now = performance.now();
    while (downs.length && now - downs[0] > 1000) downs.shift();
    out.count.textContent = String(presses);
    out.rate.textContent = String(downs.length);
    out.gap.textContent = ms(bestGap);
    out.hold.textContent = ms(bestHold);
  }

  function draw() {
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight;
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const now = performance.now();
    const left = 78, right = w - 12, laneH = (h - 26) / MAX_LANES;
    const x = (t) => right - ((now - t) / WINDOW_MS) * (right - left);
    // seconds grid
    ctx.font = "12px " + getComputedStyle(document.body).fontFamily;
    ctx.fillStyle = col.muted; ctx.strokeStyle = col.line; ctx.lineWidth = 1;
    for (let s = 0; s <= 3; s++) {
      const gx = Math.round(right - (s * 1000 / WINDOW_MS) * (right - left)) + 0.5;
      ctx.beginPath(); ctx.moveTo(gx, 4); ctx.lineTo(gx, h - 22); ctx.stroke();
      ctx.textAlign = "center"; ctx.fillText(s === 0 ? "now" : "-" + s + " s", gx, h - 6);
    }
    lanes.forEach((l, i) => {
      const y = 6 + i * laneH, bh = Math.max(10, laneH - 14);
      ctx.textAlign = "right"; ctx.fillStyle = col.text;
      ctx.fillText(l.label, left - 12, y + bh / 2 + 4);
      ctx.fillStyle = col.teal;
      l.bars = l.bars.filter((b) => b.up === null || now - b.up < WINDOW_MS);
      l.bars.forEach((b) => {
        const x0 = Math.max(left, x(b.down)), x1 = x(b.up === null ? now : b.up);
        if (x1 > left) ctx.fillRect(x0, y + 2, Math.max(2, x1 - x0), bh);
      });
    });
    stats();
    const busy = lanes.some((l) => l.bars.length);
    if (busy && running) raf = requestAnimationFrame(draw);
    else raf = 0;
  }
  function start() { running = true; if (!raf) raf = requestAnimationFrame(draw); }
  function stop() { running = false; cancelAnimationFrame(raf); raf = 0; releaseAll(); }

  window.addEventListener("keydown", onDown);
  window.addEventListener("keyup", onUp);
  window.addEventListener("blur", () => { releaseAll(); });
  return { start: () => { running = true; draw(); }, stop };
}

function demoTabs() {
  const tabs = [...document.querySelectorAll('.tabs [role="tab"]')];
  if (!tabs.length) return;
  let liveOn = false;
  const live = liveKeyboard(() => liveOn);
  const select = (tab) => {
    tabs.forEach((t) => {
      const on = t === tab;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
      document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
    });
    liveOn = tab.id === "tab-live";
    if (liveOn) live.start(); else live.stop();
  };
  tabs.forEach((t, i) => {
    t.addEventListener("click", () => select(t));
    t.addEventListener("keydown", (e) => {
      if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
      const next = tabs[(i + (e.key === "ArrowRight" ? 1 : tabs.length - 1)) % tabs.length];
      select(next); next.focus(); e.preventDefault();
    });
  });
}

rapidTriggerDemo();
demoTabs();
latestRelease();
