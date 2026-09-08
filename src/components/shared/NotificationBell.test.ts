import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

describe('NotificationBell navigation', () => {
  it('jumps to the matching agent on Connections or Agents', () => {
    const source = readFileSync(path.join(dir, 'NotificationBell.tsx'), 'utf8');
    expect(source).toContain('alert.agentId');
    expect(source).toContain('`/connections${agentQuery}`');
    expect(source).toContain('`/agents${agentQuery}`');
    expect(source).toContain('encodeURIComponent(alert.agentId)');
  });
});
