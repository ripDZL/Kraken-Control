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

rapidTriggerDemo();
latestRelease();
