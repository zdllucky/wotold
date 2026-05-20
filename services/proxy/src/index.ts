import { Hono } from 'hono';
import type { Env } from './lib/env.js';
import { authRoutes } from './routes/auth.js';
import { sttRoutes } from './routes/stt.js';
import { llmRoutes } from './routes/llm.js';
import { usageRoutes } from './routes/usage.js';

const app = new Hono<{ Bindings: Env }>();

app.get('/', (c) => c.text('wotold-proxy ok'));
app.get('/health', (c) => c.json({ ok: true, tier: 'free' as const }));

app.route('/v1/auth', authRoutes);
app.route('/v1/stt', sttRoutes);
app.route('/v1/llm', llmRoutes);
app.route('/v1/usage', usageRoutes);

// TODO(paid): отдельный пайплайн для платных тиров (R5).

export default app;
