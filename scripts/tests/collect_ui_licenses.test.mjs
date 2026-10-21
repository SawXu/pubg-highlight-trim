import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { collect } from '../collect_ui_licenses.mjs';
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function fixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ui-license-'));
  const ui = path.join(root, 'ui'), bundle = path.join(root, 'bundle');
  const put = (name, value) => { const file = path.join(root, name); fs.mkdirSync(path.dirname(file), { recursive: true }); fs.writeFileSync(file, typeof value === 'string' ? value : JSON.stringify(value)); return file; };
  put('ui/package.json', { dependencies: { runtime: '1.0.0' }, devDependencies: { tool: '1.0.0' } });
  const lock = { lockfileVersion: 3, packages: { '': { dependencies: { runtime: '1.0.0' } }, 'node_modules/runtime': { version: '1.0.0', license: 'MIT', dependencies: { child: '1.0.0' } }, 'node_modules/child': { version: '1.0.0', license: 'MIT' }, 'node_modules/tool': { version: '1.0.0', dev: true } } };
  put('ui/package-lock.json', lock); put('ui/src-tauri/Cargo.lock', '# fixture lock');
  for (const name of ['runtime', 'child']) { put(`ui/node_modules/${name}/package.json`, { name, version: '1.0.0', license: 'MIT' }); put(`ui/node_modules/${name}/LICENSE`, `fixture license ${name}`); }
  const pkg = (name, macro = false) => ({ id: name, name, version: '1.0.0', license: 'MIT', manifest_path: put(`crates/${name}/Cargo.toml`, '# fixture'), targets: [{ kind: [macro ? 'proc-macro' : 'lib'] }] });
  const packages = [pkg('pubg-highlight-trim-ui'), pkg('tauri'), pkg('macro', true), pkg('builder'), pkg('dev')];
  for (const name of ['tauri', 'macro', 'builder']) put(`crates/${name}/LICENSE`, `fixture Rust license ${name}`);
  const dep = (name, kind = null) => ({ pkg: name, dep_kinds: [{ kind }] });
  const metadata = { packages, resolve: { nodes: [{ id: 'pubg-highlight-trim-ui', deps: [dep('tauri'), dep('macro'), dep('builder', 'build'), dep('dev', 'dev')] }, ...['tauri', 'macro', 'builder', 'dev'].map(id => ({ id, deps: [] }))] } };
  return { root, ui, bundle, put, lock, metadata, options: { ui, bundle, metadata }, close: () => fs.rmSync(root, { recursive: true, force: true }) };
}

test('missing shipped npm notice fails closed before creating release inventory', async () => {
  const f = fixture(); try {
    fs.rmSync(path.join(f.ui, 'node_modules/runtime/LICENSE'));
    await assert.rejects(() => collect(f.options), /missing notice.*npm.*runtime/i);
    assert.equal(fs.existsSync(path.join(f.bundle, 'third_party/ui')), false);
  } finally { f.close(); }
});
test('collects npm transitive runtime and Rust normal/build closure, excludes dev, stable notice hashes', async () => {
  const f = fixture(); try {
    const first = await collect(f.options);
    assert.deepEqual(first.components.map(p => [p.name, p.scope]).sort(), [['child', 'shipped'], ['runtime', 'shipped'], ['builder', 'build-only'], ['macro', 'build-only'], ['tauri', 'shipped']].sort());
    for (const component of first.components) for (const notice of component.notices) assert.equal(hash(fs.readFileSync(path.join(f.bundle, notice.path))), notice.sha256);
    f.put('bundle/third_party/ui/stale.txt', 'stale');
    assert.deepEqual(await collect(f.options), first);
    assert.equal(fs.existsSync(path.join(f.bundle, 'third_party/ui/stale.txt')), false);
  } finally { f.close(); }
});
test('missing Rust transitive notices or SPDX metadata rejects without rewriting previous valid output', async () => {
  const f = fixture(); try {
    await collect(f.options);
    const before = fs.readFileSync(path.join(f.bundle, 'third_party/ui/UI_LICENSE_INVENTORY.json'), 'utf8');
    fs.rmSync(path.join(f.root, 'crates/tauri/LICENSE'));
    await assert.rejects(() => collect(f.options), /missing notice.*rust tauri/);
    assert.equal(fs.readFileSync(path.join(f.bundle, 'third_party/ui/UI_LICENSE_INVENTORY.json'), 'utf8'), before);
    f.put('crates/tauri/LICENSE', 'restored fixture'); f.metadata.packages.find(p => p.name === 'tauri').license = null;
    await assert.rejects(() => collect(f.options), /missing license metadata.*tauri/);
  } finally { f.close(); }
});
test('explicit license_file is copied even when filename is not LICENSE', async () => {
  const f = fixture(); try {
    fs.rmSync(path.join(f.root, 'crates/tauri/LICENSE'));
    f.put('crates/tauri/legal/terms.txt', 'fixture actual license');
    f.metadata.packages.find(p => p.name === 'tauri').license_file = 'legal/terms.txt';
    const result = await collect(f.options);
    assert.ok(result.components.find(p => p.name === 'tauri').notices[0].path.endsWith('legal/terms.txt'));
  } finally { f.close(); }
});
test('pinned upstream supplements require exact crate revision and text hash', async () => {
  const f = fixture(); try {
    const moduleFile = path.join(f.root, 'collector.mjs');
    fs.copyFileSync(new URL('../collect_ui_licenses.mjs', import.meta.url), moduleFile);
    fs.cpSync(new URL('../ui-license-notices/', import.meta.url), path.join(f.root, 'ui-license-notices'), { recursive: true });
    const { collect: copiedCollect } = await import((await import('node:url')).pathToFileURL(moduleFile).href);
    const pkg = f.metadata.packages.find(p => p.name === 'tauri');
    pkg.name = 'alloc-stdlib'; pkg.version = '0.3.0';
    fs.rmSync(path.join(f.root, 'crates/tauri/LICENSE'));
    f.put('crates/tauri/.cargo_vcs_info.json', { git: { sha1: '0a81fd6928ea3b33c8cd484aa4575d50ffb98012' } });
    const result = await copiedCollect(f.options);
    assert.ok(result.components.find(p => p.name === 'alloc-stdlib').supplemental_source);
    f.put('ui-license-notices/alloc-stdlib-LICENSE', 'tampered');
    await assert.rejects(() => copiedCollect(f.options), /supplement notice SHA256 mismatch/);
    f.put('crates/tauri/.cargo_vcs_info.json', { git: { sha1: 'wrong' } });
    await assert.rejects(() => copiedCollect(f.options), /supplement crate revision mismatch/);
  } finally { f.close(); }
});

test('Fixed requires reviewed trusted manifest, Microsoft source, original hashed text, and exact complete runtime bytes', async () => {
  const f = fixture(); try {
    const runtime = path.join(f.root, 'runtime'); f.put('runtime/msedgewebview2.exe', 'not-a-real-runtime');
    const options = { ...f.options, variant: 'Fixed', runtime };
    await assert.rejects(() => collect(options), /reviewed license manifest/);
    f.put('evidence/license.txt', 'Fixture terms, not Microsoft license text.');
    const evidence = { schema: 'pubg-highlight-trim/fixed-runtime-license/v1', version: 'fixture', source: 'https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution', reviewed_by: 'fixture reviewer', obligations: 'fixture review, not legal approval', runtime_files: [{ path: 'msedgewebview2.exe', sha256: hash('not-a-real-runtime') }], notices: [{ path: 'license.txt', source: 'https://www.microsoft.com/example-fixture', sha256: hash('Fixture terms, not Microsoft license text.') }] };
    const update = () => { options.evidence = f.put('evidence/manifest.json', evidence); options.evidenceSha256 = hash(fs.readFileSync(options.evidence)); };
    update(); const result = await collect(options); assert.equal(result.components.at(-1).ecosystem, 'runtime');
    evidence.source = 'https://example.org/untrusted'; update(); await assert.rejects(() => collect(options), /authoritative Microsoft/);
    evidence.source = 'https://learn.microsoft.com/example-fixture'; evidence.obligations = ''; update(); await assert.rejects(() => collect(options), /obligations/);
    evidence.obligations = 'fixture'; evidence.notices[0].sha256 = '0'.repeat(64); update(); await assert.rejects(() => collect(options), /notice SHA256 mismatch/);
    evidence.notices[0].sha256 = hash('Fixture terms, not Microsoft license text.'); update();
    f.put('runtime/extra.dll', 'unreviewed'); await assert.rejects(() => collect(options), /inventory mismatch/);
    fs.rmSync(path.join(runtime, 'extra.dll')); f.put('runtime/msedgewebview2.exe', 'tampered'); await assert.rejects(() => collect(options), /runtime hash mismatch/);
    options.evidenceSha256 = '0'.repeat(64); await assert.rejects(() => collect(options), /manifest SHA256 mismatch/);
  } finally { f.close(); }
});
