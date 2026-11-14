// scripts/run-unit.mjs — node --test over TS sources via type stripping.
import { spawnSync } from 'node:child_process';

const [major, minor] = process.versions.node.split('.').map(Number);
const supportsTs = major > 22 || (major === 22 && minor >= 6);
if (!supportsTs) {
  console.log(
    `SKIP unit tests: node ${process.version} lacks --experimental-strip-types (need >= 22.6)`,
  );
  process.exit(0);
}
const r = spawnSync(
  process.execPath,
  ['--experimental-strip-types', '--test', 'tests/signal.test.ts'],
  { stdio: 'inherit' },
);
process.exit(r.status ?? 1);
