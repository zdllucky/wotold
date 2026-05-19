import type { Context, Next } from 'hono';
import { DEVICE_ID_HEADER } from '@wotold/contracts';

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

declare module 'hono' {
  interface ContextVariableMap {
    deviceId: string;
  }
}

export async function requireDeviceId(c: Context, next: Next): Promise<Response | void> {
  const raw = c.req.header(DEVICE_ID_HEADER);
  if (!raw || !UUID_RE.test(raw)) {
    return c.json(
      {
        ok: false,
        code: 'invalid_device_id',
        message: `${DEVICE_ID_HEADER} header missing or not a UUID`,
      },
      400,
    );
  }
  c.set('deviceId', raw.toLowerCase());
  await next();
}
