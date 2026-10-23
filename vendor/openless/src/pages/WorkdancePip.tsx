import { useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';

type Landmark = { x: number; y: number; z?: number };

type HandPayload = {
  handCount: number;
  gateState: string;
  landmarks: Landmark[] | null;
  accessibilityDenied?: boolean;
  labelZh?: string;
  /** Base64 BMP from the Rust-owned camera (same frames as MediaPipe). */
  frameJpeg?: string | null;
};

const HAND_EDGES: Array<[number, number]> = [
  [0, 1], [1, 2], [2, 3], [3, 4],
  [0, 5], [5, 6], [6, 7], [7, 8],
  [0, 9], [9, 10], [10, 11], [11, 12],
  [0, 13], [13, 14], [14, 15], [15, 16],
  [0, 17], [17, 18], [18, 19], [19, 20],
  [5, 9], [9, 13], [13, 17],
];
const FINGERTIPS = new Set([4, 8, 12, 16, 20]);
const ALPHA = 0.4;

function trayLabel(state: string): string {
  switch (state) {
    case 'tracking':
      return '正在听手';
    case 'recording':
      return '正在录音';
    case 'reject_cooldown':
      return '刚误触停一下';
    case 'not_installed':
      return '手势没装好';
    case 'needs_accessibility':
      return '需要辅助功能';
    default:
      return '';
  }
}

export function WorkdancePip() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const previewRef = useRef<HTMLImageElement>(null);
  const smoothRef = useRef<Array<{ x: number; y: number }> | null>(null);
  const [count, setCount] = useState(0);
  const [status, setStatus] = useState('');
  const [perm, setPerm] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    let objectUrl: string | null = null;
    (async () => {
      unlisten = await listen<HandPayload>('workdance:hand', (ev) => {
        if (cancelled) return;
        const payload = ev.payload;
        const handCount = payload?.handCount ?? 0;
        setCount(handCount);
        const state = payload?.gateState || 'idle';
        setStatus(payload?.labelZh || trayLabel(state));
        const access =
          state === 'needs_accessibility' || payload?.accessibilityDenied === true;
        setPerm(access);

        const jpeg = payload?.frameJpeg;
        if (jpeg && previewRef.current) {
          const next = `data:image/bmp;base64,${jpeg}`;
          // Assign via src; browser caches identical data URLs cheaply enough at ~30fps.
          if (previewRef.current.src !== next) {
            previewRef.current.src = next;
          }
          if (objectUrl) {
            URL.revokeObjectURL(objectUrl);
            objectUrl = null;
          }
        }

        const canvas = canvasRef.current;
        const ctx = canvas?.getContext('2d');
        if (!canvas || !ctx) return;
        ctx.clearRect(0, 0, canvas.width, canvas.height);
        if (handCount <= 0 || !payload?.landmarks?.length) {
          smoothRef.current = null;
          return;
        }
        // Preview CSS-mirrored; mirror landmarks to match.
        const pts = payload.landmarks.map((p) => ({
          x: (1 - p.x) * canvas.width,
          y: p.y * canvas.height,
        }));
        const prev = smoothRef.current;
        const draw =
          prev && prev.length === pts.length
            ? pts.map((p, i) => ({
                x: prev[i].x + (p.x - prev[i].x) * ALPHA,
                y: prev[i].y + (p.y - prev[i].y) * ALPHA,
              }))
            : pts;
        smoothRef.current = draw;
        ctx.lineWidth = 2;
        ctx.strokeStyle = 'rgba(59,130,246,0.95)';
        ctx.lineCap = 'round';
        ctx.beginPath();
        for (const [a, b] of HAND_EDGES) {
          if (!draw[a] || !draw[b]) continue;
          ctx.moveTo(draw[a].x, draw[a].y);
          ctx.lineTo(draw[b].x, draw[b].y);
        }
        ctx.stroke();
        for (let i = 0; i < draw.length; i++) {
          const p = draw[i];
          if (!p) continue;
          const tip = FINGERTIPS.has(i);
          ctx.beginPath();
          ctx.fillStyle = tip ? 'rgba(147,197,253,1)' : 'rgba(96,165,250,0.95)';
          ctx.arc(p.x, p.y, tip ? 3.4 : 2.1, 0, Math.PI * 2);
          ctx.fill();
          if (tip) {
            const g = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, 9);
            g.addColorStop(0, 'rgba(147,197,253,0.55)');
            g.addColorStop(1, 'transparent');
            ctx.fillStyle = g;
            ctx.beginPath();
            ctx.arc(p.x, p.y, 9, 0, Math.PI * 2);
            ctx.fill();
          }
        }
      });
    })().catch((err) => console.warn('[workdance-pip] listen', err));
    return () => {
      cancelled = true;
      unlisten?.();
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, []);

  return (
    <div
      style={{
        margin: 0,
        padding: 0,
        width: '100%',
        height: '100%',
        overflow: 'hidden',
        background: 'transparent',
      }}
    >
      <div
        style={{
          position: 'relative',
          width: 180,
          height: 120,
          borderRadius: 10,
          overflow: 'hidden',
          border: '1px solid rgba(255,255,255,0.14)',
          background: '#0a0a0c',
          boxShadow: '0 8px 28px rgba(0,0,0,0.45)',
        }}
      >
        <img
          ref={previewRef}
          alt=""
          style={{
            position: 'absolute',
            inset: 0,
            width: '100%',
            height: '100%',
            objectFit: 'cover',
            transform: 'scaleX(-1)',
            background: '#0a0a0c',
          }}
        />
        <canvas
          ref={canvasRef}
          width={180}
          height={120}
          aria-hidden
          style={{
            position: 'absolute',
            inset: 0,
            width: '100%',
            height: '100%',
            zIndex: 2,
            pointerEvents: 'none',
            opacity: 0.95,
          }}
        />
        <div
          style={{
            position: 'absolute',
            zIndex: 3,
            left: 8,
            top: 6,
            font: '600 10px/1.2 ui-sans-serif, system-ui, sans-serif',
            color: 'rgba(230,235,245,0.8)',
            textShadow: '0 1px 4px rgba(0,0,0,0.7)',
            letterSpacing: '0.04em',
          }}
        >
          前置
        </div>
        <div
          style={{
            position: 'absolute',
            zIndex: 3,
            right: 8,
            top: 6,
            minWidth: 16,
            padding: '1px 5px',
            borderRadius: 999,
            font: '700 10px/1.3 ui-sans-serif, system-ui, sans-serif',
            color: '#fff',
            background: 'rgba(20,24,33,0.72)',
            textAlign: 'center',
          }}
        >
          {count}
        </div>
        <div
          style={{
            position: 'absolute',
            zIndex: 3,
            left: 8,
            bottom: 7,
            font: '600 10px/1.2 ui-sans-serif, system-ui, sans-serif',
            color: 'rgba(147,197,253,0.95)',
            textShadow: '0 1px 4px rgba(0,0,0,0.75)',
          }}
        >
          {status}
        </div>
        {perm ? (
          <div
            style={{
              position: 'absolute',
              zIndex: 4,
              inset: 0,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              padding: 10,
              textAlign: 'center',
              background: 'rgba(8,10,14,0.88)',
              color: '#fde68a',
              font: '600 11px/1.35 ui-sans-serif, system-ui, sans-serif',
            }}
          >
            请在系统设置里打开「辅助功能」，否则鼠标跟手无效
          </div>
        ) : null}
      </div>
    </div>
  );
}
