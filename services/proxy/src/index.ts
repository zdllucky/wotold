import { Hono } from 'hono';
import { cors } from 'hono/cors';
import type { Env } from './lib/env.js';
import { authRoutes } from './routes/auth.js';
import { sttRoutes } from './routes/stt.js';
import { llmRoutes } from './routes/llm.js';
import { usageRoutes } from './routes/usage.js';

const app = new Hono<{ Bindings: Env }>();

// [B16 audit P1]: CORS — раньше origin: '*', теперь allowlist.
// Tauri webview шлёт fetch с одним из:
//   - 'tauri://localhost'    macOS / Linux production
//   - 'http://tauri.localhost' Windows production
//   - 'http://localhost:5173' / 'http://127.0.0.1:5173' vite dev
// Auth — Bearer token либо x-device-id UUID, не cookie. Но даже без
// credentials явный allowlist предотвращает scenarios где malicious site
// сможет читать /v1/auth/me responses (через XSS на trusted domain).
//
// '/health' и '/' оставляем открытыми (smoke checks из CI/Cloudflare).
const ALLOWED_ORIGINS = new Set([
  'tauri://localhost',
  'http://tauri.localhost',
  'http://localhost:5173',
  'http://127.0.0.1:5173',
]);

function originAllowlist(origin: string | undefined): string | null {
  if (!origin) return null;
  return ALLOWED_ORIGINS.has(origin) ? origin : null;
}

app.use(
  '/v1/*',
  cors({
    origin: originAllowlist,
    allowMethods: ['GET', 'POST', 'OPTIONS'],
    allowHeaders: ['content-type', 'authorization', 'x-device-id'],
    maxAge: 86400,
  }),
);

app.get('/', (c) => c.text('wotold-proxy ok'));
app.get('/health', (c) => c.json({ ok: true, tier: 'free' as const }));

app.route('/v1/auth', authRoutes);
app.route('/v1/stt', sttRoutes);
app.route('/v1/llm', llmRoutes);
app.route('/v1/usage', usageRoutes);

// TODO(paid): отдельный пайплайн для платных тиров (R5).

export default app;
