import { Hono } from 'hono';
import { cors } from 'hono/cors';
import type { Env } from './lib/env.js';
import { authRoutes } from './routes/auth.js';
import { sttRoutes } from './routes/stt.js';
import { llmRoutes } from './routes/llm.js';
import { usageRoutes } from './routes/usage.js';

const app = new Hono<{ Bindings: Env }>();

// CORS: Tauri webview шлёт fetch с origin 'tauri://localhost' (macOS/Linux),
// 'http://tauri.localhost' (Windows), либо 'http://localhost:5173' (vite dev).
// Без cookies/credentials → '*' безопасен: каждый запрос требует валидный
// x-device-id UUID или Bearer session, contentу не доверяем по origin'у.
// M9.5 паспорта (auth по device-id, не по cookie).
app.use(
  '*',
  cors({
    origin: '*',
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
