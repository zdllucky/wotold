import { describe, expect, test } from 'vitest';
import { Hono } from 'hono';
import { DEVICE_ID_HEADER } from '@wotold/contracts';
import { requireDeviceId } from './device-id.js';

function buildApp() {
  const app = new Hono<{ Variables: { deviceId: string } }>();
  app.use('*', requireDeviceId);
  app.get('/', (c) => c.json({ deviceId: c.get('deviceId') }));
  return app;
}

const VALID_UUID = '550e8400-e29b-41d4-a716-446655440000';

describe('requireDeviceId middleware', () => {
  test('rejects request without device-id header', async () => {
    const res = await buildApp().request('/');
    expect(res.status).toBe(400);
    const body = (await res.json()) as { code: string };
    expect(body.code).toBe('invalid_device_id');
  });

  test('rejects malformed device-id', async () => {
    const res = await buildApp().request('/', {
      headers: { [DEVICE_ID_HEADER]: 'not-a-uuid' },
    });
    expect(res.status).toBe(400);
  });

  test('rejects empty header', async () => {
    const res = await buildApp().request('/', { headers: { [DEVICE_ID_HEADER]: '' } });
    expect(res.status).toBe(400);
  });

  test('accepts valid lowercase UUID', async () => {
    const res = await buildApp().request('/', { headers: { [DEVICE_ID_HEADER]: VALID_UUID } });
    expect(res.status).toBe(200);
    const body = (await res.json()) as { deviceId: string };
    expect(body.deviceId).toBe(VALID_UUID);
  });

  test('normalizes uppercase UUID to lowercase', async () => {
    const upper = VALID_UUID.toUpperCase();
    const res = await buildApp().request('/', { headers: { [DEVICE_ID_HEADER]: upper } });
    expect(res.status).toBe(200);
    const body = (await res.json()) as { deviceId: string };
    expect(body.deviceId).toBe(VALID_UUID);
  });
});
