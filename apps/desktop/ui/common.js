const invoke = (cmd, args = {}) => {
  if (window.__TAURI__?.core?.invoke) {
    return window.__TAURI__.core.invoke(cmd, args);
  }
  // Browser preview fallback (static server / CI screenshot)
  return Promise.reject(new Error("tauri unavailable"));
};

function toast(msg) {
  let el = document.querySelector(".toast");
  if (!el) {
    el = document.createElement("div");
    el.className = "toast";
    document.body.appendChild(el);
  }
  el.textContent = msg;
  el.classList.add("show");
  clearTimeout(el._t);
  el._t = setTimeout(() => el.classList.remove("show"), 1800);
}

function bindSeg(root, onChange) {
  root.querySelectorAll("button").forEach((btn) => {
    btn.addEventListener("click", () => {
      root.querySelectorAll("button").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      onChange(btn.dataset.value);
    });
  });
}

function setSeg(root, value) {
  root.querySelectorAll("button").forEach((b) => {
    b.classList.toggle("active", b.dataset.value === value);
  });
}

/** MediaPipe-ish 21-point layout in pixel space (for faint PiP skeleton / mock tip). */
function mockHandLandmarks(w, h) {
  const sx = w * 0.52;
  const sy = h * 0.62;
  const s = Math.min(w, h) * 0.38;
  const base = [
    [0.0, 0.35],
    [-0.12, 0.22],
    [-0.18, 0.08],
    [-0.2, -0.05],
    [-0.18, -0.16],
    [-0.08, 0.05],
    [-0.06, -0.12],
    [-0.04, -0.26],
    [-0.02, -0.38],
    [0.02, 0.06],
    [0.05, -0.14],
    [0.07, -0.28],
    [0.08, -0.4],
    [0.1, 0.1],
    [0.14, -0.08],
    [0.16, -0.2],
    [0.17, -0.3],
    [0.16, 0.16],
    [0.2, 0.02],
    [0.22, -0.08],
    [0.23, -0.16],
  ];
  return base.map(([x, y]) => ({ x: sx + x * s * 1.1, y: sy + y * s }));
}

const HAND_EDGES = [
  [0, 1], [1, 2], [2, 3], [3, 4],
  [0, 5], [5, 6], [6, 7], [7, 8],
  [0, 9], [9, 10], [10, 11], [11, 12],
  [0, 13], [13, 14], [14, 15], [15, 16],
  [0, 17], [17, 18], [18, 19], [19, 20],
  [5, 9], [9, 13], [13, 17],
];

function drawFaintSkeleton(ctx, pts, opts = {}) {
  const accent = opts.accent || "rgba(59,130,246,0.75)";
  const tipIndex = opts.tipIndex ?? 8;
  ctx.save();
  ctx.lineWidth = 1.2;
  ctx.strokeStyle = accent.replace("0.75", "0.35").replace(/rgba?\(([^)]+)\)/, (_, inner) => {
    if (accent.startsWith("#")) return accent + "55";
    return `rgba(${inner})`;
  });
  if (accent.startsWith("#")) ctx.strokeStyle = accent + "66";
  ctx.beginPath();
  for (const [a, b] of HAND_EDGES) {
    if (!pts[a] || !pts[b]) continue;
    ctx.moveTo(pts[a].x, pts[a].y);
    ctx.lineTo(pts[b].x, pts[b].y);
  }
  ctx.stroke();
  for (let i = 0; i < pts.length; i++) {
    const p = pts[i];
    if (!p) continue;
    const r = i === tipIndex ? 3.5 : 1.8;
    ctx.beginPath();
    ctx.fillStyle = i === tipIndex ? accent : (accent.startsWith("#") ? accent + "99" : accent);
    ctx.arc(p.x, p.y, r, 0, Math.PI * 2);
    ctx.fill();
  }
  if (pts[tipIndex]) {
    const p = pts[tipIndex];
    const g = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, 10);
    g.addColorStop(0, accent.startsWith("#") ? accent + "aa" : accent);
    g.addColorStop(1, "transparent");
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.arc(p.x, p.y, 10, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.restore();
}

/** Stylized front-camera hand plate for demo / no-device screenshots. */
function drawDemoHand(ctx, w, h, opts = {}) {
  ctx.clearRect(0, 0, w, h);
  const g = ctx.createRadialGradient(w * 0.5, h * 0.45, 0, w * 0.5, h * 0.5, Math.max(w, h) * 0.7);
  g.addColorStop(0, "#2a221c");
  g.addColorStop(0.55, "#14110f");
  g.addColorStop(1, "#070605");
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, w, h);

  const compact = !!opts.compact;
  const cx = w * (compact ? 0.52 : 0.42);
  const cy = h * (compact ? 0.58 : 0.55);
  const s = Math.min(w, h) * (compact ? 0.42 : 0.48);

  ctx.save();
  ctx.translate(cx, cy);
  ctx.scale(s, s);
  ctx.fillStyle = "#c4a484";
  ctx.beginPath();
  // palm
  ctx.moveTo(-0.22, 0.05);
  ctx.quadraticCurveTo(-0.28, 0.35, -0.08, 0.55);
  ctx.quadraticCurveTo(0.12, 0.62, 0.28, 0.4);
  ctx.quadraticCurveTo(0.34, 0.15, 0.22, -0.02);
  ctx.closePath();
  ctx.fill();

  const fingers = [
    { x: -0.18, lean: -0.08, len: 0.42 },
    { x: -0.06, lean: -0.02, len: 0.52 }, // index
    { x: 0.06, lean: 0.02, len: 0.5 },
    { x: 0.16, lean: 0.06, len: 0.44 },
    { x: 0.24, lean: 0.12, len: 0.32 },
  ];
  fingers.forEach((f, i) => {
    ctx.beginPath();
    ctx.moveTo(f.x - 0.035, 0);
    ctx.quadraticCurveTo(f.x + f.lean * 0.4, -f.len * 0.55, f.x + f.lean - 0.01, -f.len);
    ctx.quadraticCurveTo(f.x + f.lean + 0.04, -f.len * 0.55, f.x + 0.04, 0.02);
    ctx.closePath();
    ctx.fillStyle = i === 1 ? "#d0b090" : "#c4a484";
    ctx.fill();
  });
  ctx.restore();

  // Index tip highlight (mock landmark 8)
  const tipX = cx + (-0.06 + -0.02) * s;
  const tipY = cy - 0.52 * s;
  const glow = ctx.createRadialGradient(tipX, tipY, 0, tipX, tipY, s * 0.08);
  glow.addColorStop(0, "rgba(96,165,250,0.95)");
  glow.addColorStop(0.45, "rgba(59,130,246,0.45)");
  glow.addColorStop(1, "transparent");
  ctx.fillStyle = glow;
  ctx.beginPath();
  ctx.arc(tipX, tipY, s * 0.08, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#93c5fd";
  ctx.beginPath();
  ctx.arc(tipX, tipY, Math.max(2.5, s * 0.018), 0, Math.PI * 2);
  ctx.fill();
}

/**
 * Attach mirrored front preview. Uses getUserMedia when available;
 * demo canvas only when forceDemo (screenshot / offline). Does not touch DualTier.
 */
async function attachFrontPreview({ video, canvas, forceDemo = false, drawDemo }) {
  const tryLive = !forceDemo && navigator.mediaDevices?.getUserMedia;
  if (tryLive) {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: "user", width: { ideal: 1280 }, height: { ideal: 720 } },
        audio: false,
      });
      if (video) {
        video.srcObject = stream;
        video.hidden = false;
        if (canvas) canvas.hidden = true;
        await video.play().catch(() => {});
        return { ok: true, kind: "camera" };
      }
    } catch {
      /* fall through to empty (or demo if forced) */
    }
  }

  if (forceDemo && canvas && drawDemo) {
    canvas.hidden = false;
    if (video) video.hidden = true;
    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      const w = Math.max(2, Math.round(rect.width * (window.devicePixelRatio || 1)));
      const h = Math.max(2, Math.round(rect.height * (window.devicePixelRatio || 1)));
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      drawDemo(canvas.getContext("2d"), canvas.width, canvas.height);
    };
    resize();
    window.addEventListener("resize", resize);
    return { ok: true, kind: "demo" };
  }

  if (video) video.hidden = true;
  if (canvas) canvas.hidden = true;
  return { ok: false, kind: "empty" };
}

window.WD = {
  invoke,
  toast,
  bindSeg,
  setSeg,
  mockHandLandmarks,
  drawFaintSkeleton,
  drawDemoHand,
  attachFrontPreview,
};
