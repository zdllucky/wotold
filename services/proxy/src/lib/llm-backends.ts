// LLM backend adapters для /v1/llm route.
//
// Унифицирует Anthropic Messages API и OpenAI-compatible API (Groq).
// Выбор backend через env.LLM_BACKEND или auto-pick по наличию ключей.
//
// Контракт callLlm: вернуть либо parsed JSON либо ошибку с code/message.

import type { Env } from './env.js';
import type { ProxyErrorCode } from '@wotold/contracts';

const ANTHROPIC_URL = 'https://api.anthropic.com/v1/messages';
const GROQ_URL = 'https://api.groq.com/openai/v1/chat/completions';
const GROQ_DEFAULT_MODEL_FALLBACK = 'llama-3.3-70b-versatile';

/** Retry upstream LLM один раз при HTTP 5xx — покрывает transient glitches
 *  и rate-limit «retry-after» через паузу 1.5s. ([B12]) */
async function fetchWithRetry(url: string, init: RequestInit): Promise<Response> {
  const first = await fetch(url, init);
  if (first.status >= 500 || first.status === 429) {
    await new Promise((r) => setTimeout(r, 1500));
    return fetch(url, init);
  }
  return first;
}

export interface LlmCallInput {
  system: string;
  input: string;
  /** Optional model override от клиента. */
  model?: string;
  maxTokens?: number;
}

export interface LlmCallOk {
  ok: true;
  parsed: unknown;
  tokensUsed: number;
}

export interface LlmCallErr {
  ok: false;
  status: number;
  code: ProxyErrorCode;
  message: string;
}

export type LlmCallResult = LlmCallOk | LlmCallErr;

export type LlmBackend = 'anthropic' | 'groq';

/** auto-pick: если LLM_BACKEND явно задан — он. Иначе groq если GROQ_API_KEY есть, anthropic иначе. */
export function resolveBackend(env: Env): LlmBackend | null {
  const explicit = (env.LLM_BACKEND ?? '').trim().toLowerCase();
  if (explicit === 'anthropic') return 'anthropic';
  if (explicit === 'groq') return 'groq';
  if (env.GROQ_API_KEY) return 'groq';
  if (env.ANTHROPIC_API_KEY) return 'anthropic';
  return null;
}

export async function callLlm(env: Env, body: LlmCallInput): Promise<LlmCallResult> {
  const backend = resolveBackend(env);
  if (backend === null) {
    return {
      ok: false,
      status: 503,
      code: 'provider_error',
      message: 'No LLM provider configured (set GROQ_API_KEY or ANTHROPIC_API_KEY)',
    };
  }
  return backend === 'anthropic' ? callAnthropic(env, body) : callGroq(env, body);
}

async function callAnthropic(env: Env, body: LlmCallInput): Promise<LlmCallResult> {
  if (!env.ANTHROPIC_API_KEY) {
    return {
      ok: false,
      status: 503,
      code: 'provider_error',
      message: 'Anthropic key not configured',
    };
  }
  const model = body.model ?? env.ANTHROPIC_DEFAULT_MODEL;
  const upstream = await fetchWithRetry(ANTHROPIC_URL, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      'x-api-key': env.ANTHROPIC_API_KEY,
      'anthropic-version': '2023-06-01',
    },
    body: JSON.stringify({
      model,
      max_tokens: body.maxTokens ?? 4096,
      system: body.system,
      messages: [{ role: 'user', content: body.input }],
    }),
  });

  if (!upstream.ok) {
    const text = await upstream.text();
    // [B16 audit P2]: full upstream body — только в логи (Cloudflare console),
    // юзеру отдаём generic — иначе утечка provider error details может выдать
    // версии моделей / lim'итов / API key fragments.
    console.error('anthropic upstream error', upstream.status, text.slice(0, 500));
    return {
      ok: false,
      status: 502,
      code: 'provider_error',
      message: `LLM upstream error (${upstream.status})`,
    };
  }

  const data = (await upstream.json()) as {
    content: Array<{ type: string; text?: string }>;
    usage?: { input_tokens: number; output_tokens: number };
  };

  const textBlock = data.content.find((b) => b.type === 'text')?.text ?? '';
  return parseAndPack(textBlock, (data.usage?.input_tokens ?? 0) + (data.usage?.output_tokens ?? 0));
}

async function callGroq(env: Env, body: LlmCallInput): Promise<LlmCallResult> {
  if (!env.GROQ_API_KEY) {
    return {
      ok: false,
      status: 503,
      code: 'provider_error',
      message: 'Groq key not configured',
    };
  }
  // Игнорируем модель если она явно anthropic/openai-формата (legacy settings),
  // иначе Groq отдаст 404 "model does not exist". Принимаем только модели,
  // которые выглядят как Groq (llama, mixtral, deepseek, gemma и т.д.).
  const userModel = body.model?.trim();
  const looksLikeGroq =
    !!userModel &&
    !/^(claude-|gpt-|o[1-9]-|gemini-)/i.test(userModel);
  const model = looksLikeGroq
    ? userModel
    : env.GROQ_DEFAULT_MODEL || GROQ_DEFAULT_MODEL_FALLBACK;

  // Groq OpenAI-compatible chat completions. response_format: json_object форсит
  // model вернуть валидный JSON — но требует "JSON" слово в messages
  // (наш build_system_prompt уже содержит JSON-схему, требование выполнено).
  const upstream = await fetchWithRetry(GROQ_URL, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      authorization: `Bearer ${env.GROQ_API_KEY}`,
    },
    body: JSON.stringify({
      model,
      max_tokens: body.maxTokens ?? 4096,
      response_format: { type: 'json_object' },
      messages: [
        { role: 'system', content: body.system },
        { role: 'user', content: body.input },
      ],
    }),
  });

  if (!upstream.ok) {
    const text = await upstream.text();
    // [B16 audit P2]: full upstream body — только в логи (Cloudflare console),
    // юзеру отдаём generic.
    console.error('groq upstream error', upstream.status, text.slice(0, 500));
    return {
      ok: false,
      status: 502,
      code: 'provider_error',
      message: `LLM upstream error (${upstream.status})`,
    };
  }

  const data = (await upstream.json()) as {
    choices: Array<{ message: { content: string } }>;
    usage?: { total_tokens?: number; prompt_tokens?: number; completion_tokens?: number };
  };

  const textBlock = data.choices[0]?.message?.content ?? '';
  const tokens =
    data.usage?.total_tokens ??
    (data.usage?.prompt_tokens ?? 0) + (data.usage?.completion_tokens ?? 0);
  return parseAndPack(textBlock, tokens);
}

function parseAndPack(text: string, tokensUsed: number): LlmCallResult {
  if (!text) {
    return {
      ok: false,
      status: 502,
      code: 'provider_error',
      message: 'LLM returned empty content',
    };
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return {
      ok: false,
      status: 502,
      code: 'provider_error',
      message: 'LLM did not return JSON-parseable output',
    };
  }
  return { ok: true, parsed, tokensUsed };
}
