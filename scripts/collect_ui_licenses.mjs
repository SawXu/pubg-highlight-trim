// Release-only, offline notice collection. No SPDX text synthesis or network fallback.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const json = file => JSON.parse(fs.readFileSync(file, 'utf8').replace(/^\uFEFF/, ''));
const slash = name => name.replaceAll('\\', '/');
const noticeRoot = fileURLToPath(new URL('./ui-license-notices/', import.meta.url));
function need(value, message) { if (!value) throw new Error(message); return value; }
function inside(root, relative) {
  need(typeof relative === 'string' && relative && !path.isAbsolute(relative) && !relative.split(/[\\/]/).includes('..'), `unsafe relative path: ${relative}`);
  const file = path.resolve(root, relative);
  need(file.startsWith(path.resolve(root) + path.sep), `path escapes root: ${relative}`);
  return file;
}
function files(root, relative = '', exclude = true) {
  const result = [];
  for (const item of fs.readdirSync(path.join(root, relative), { withFileTypes: true })) {
    const rel = slash(path.join(relative, item.name));
    need(!item.isSymbolicLink(), `symlink not allowed in notice/runtime input: ${rel}`);
    if (item.isDirectory() && (!exclude || !['node_modules', 'target', '.git'].includes(item.name))) result.push(...files(root, rel, exclude));
    else if (item.isFile()) result.push(rel);
  }
  return result.sort();
}
function notices(root, label, extra) {
  const result = files(root).filter(name => /^(license|licence|notice|copying|thirdparty.?notices)/i.test(path.basename(name)))
    .map(name => ({ name, bytes: fs.readFileSync(inside(root, name)) }));
  if (extra && !result.some(item => item.name === slash(extra))) result.push({ name: slash(extra), bytes: fs.readFileSync(inside(root, extra)) });
  need(result.some(item => item.bytes.toString('utf8').trim()), `missing notice text for ${label}`);
  return result;
}
function license(value, label) {
  need(typeof value === 'string' && value.trim() && !/^(unknown|unlicensed|none|noassertion)$/i.test(value), `missing license metadata for ${label}`);
  return value;
}
function npmPackages(ui) {
  const pkg = json(path.join(ui, 'package.json'));
  const lock = json(path.join(ui, 'package-lock.json'));
  need(lock.lockfileVersion >= 2 && lock.packages, 'npm package-lock v2/v3 required');
  const found = new Set();
  const resolve = (from, name) => {
    let dir = from;
    while (true) {
      const key = slash(path.posix.join(dir, 'node_modules', name));
      if (lock.packages[key]) return key;
      if (!dir) break;
      dir = path.posix.dirname(dir); if (dir === '.') dir = '';
    }
    throw new Error(`npm locked dependency missing: ${from} -> ${name}`);
  };
  const visit = key => {
    if (found.has(key)) return;
    found.add(key);
    const entry = lock.packages[key];
    need(!entry.link, `npm workspace/link not supported for release: ${key}`);
    for (const name of Object.keys({ ...entry.dependencies, ...entry.optionalDependencies, ...entry.peerDependencies }).sort()) visit(resolve(key, name));
  };
  for (const name of Object.keys({ ...pkg.dependencies, ...pkg.optionalDependencies }).sort()) visit(resolve('', name));
  return [...found].sort().map(key => {
    const entry = lock.packages[key];
    const dir = inside(ui, key);
    const installed = json(path.join(dir, 'package.json'));
    need(installed.version === entry.version, `npm installed/lock version mismatch: ${key}`);
    const label = `npm ${installed.name}@${entry.version}`;
    return { ecosystem: 'npm', name: installed.name, version: entry.version, scope: 'shipped', source: entry.resolved || null, integrity: entry.integrity || null,
      license: license(installed.license || entry.license, label), notices: notices(dir, label) };
  });
}
function rustPackages(metadata) {
  const nodes = new Map((metadata.resolve?.nodes || []).map(node => [node.id, node]));
  const packages = new Map((metadata.packages || []).map(pkg => [pkg.id, pkg]));
  const root = need((metadata.packages || []).find(pkg => pkg.name === 'pubg-highlight-trim-ui'), 'Tauri metadata root pubg-highlight-trim-ui missing');
  const scopes = new Map();
  function visit(id, scope) {
    if (scopes.get(id) === 'shipped' || scopes.get(id) === scope) return;
    scopes.set(id, scope);
    const node = need(nodes.get(id), `cargo resolve node missing: ${id}`);
    for (const dep of node.deps || []) {
      const kinds = dep.dep_kinds || [];
      if (!kinds.some(kind => kind.kind !== 'dev')) continue;
      const pkg = need(packages.get(dep.pkg), `cargo package missing: ${dep.pkg}`);
      const build = scope === 'build-only' || !kinds.some(kind => kind.kind === null) || pkg.targets?.some(target => target.kind.includes('proc-macro'));
      visit(dep.pkg, build ? 'build-only' : 'shipped');
    }
  }
  visit(root.id, 'shipped');
  return [...scopes].filter(([id]) => id !== root.id).sort(([a], [b]) => a.localeCompare(b)).map(([id, scope]) => {
    const pkg = packages.get(id), label = `rust ${pkg.name}@${pkg.version}`;
    const dir = path.dirname(pkg.manifest_path);
    let texts, supplemental_source;
    try { texts = notices(dir, label, pkg.license_file); }
    catch (error) {
      if (!error.message.startsWith('missing notice text')) throw error;
      const supplement = json(path.join(noticeRoot, 'SOURCES.json')).find(item => item.name === pkg.name && item.version === pkg.version);
      if (!supplement) throw error;
      need(json(path.join(dir, '.cargo_vcs_info.json')).git?.sha1 === supplement.revision, `supplement crate revision mismatch: ${label}`);
      const bytes = fs.readFileSync(inside(noticeRoot, supplement.file));
      need(sha(bytes) === supplement.sha256, `supplement notice SHA256 mismatch: ${label}`);
      texts = [{ name: supplement.file, bytes }]; supplemental_source = supplement;
    }
    return { ecosystem: 'rust', name: pkg.name, version: pkg.version, source: pkg.source || 'local source', scope, license: license(pkg.license || (pkg.license_file && 'SEE LICENSE FILE'), label), supplemental_source, notices: texts };
  });
}
function fixedEvidence(runtime, evidenceFile, expectedHash) {
  need(evidenceFile && /^[a-f0-9]{64}$/i.test(expectedHash || ''), 'Fixed requires reviewed license manifest and its trusted SHA256');
  need(sha(fs.readFileSync(evidenceFile)) === expectedHash.toLowerCase(), 'Fixed license manifest SHA256 mismatch');
  const evidence = json(evidenceFile);
  need(evidence.schema === 'pubg-highlight-trim/fixed-runtime-license/v1', 'unsupported Fixed license manifest schema');
  need(typeof evidence.version === 'string' && evidence.version.trim(), 'Fixed runtime version required');
  const microsoft = url => { try { const u = new URL(url); return u.protocol === 'https:' && (u.hostname === 'microsoft.com' || u.hostname.endsWith('.microsoft.com')); } catch { return false; } };
  need(microsoft(evidence.source), 'Fixed requires authoritative Microsoft HTTPS license source');
  need(typeof evidence.obligations === 'string' && evidence.obligations.trim() && typeof evidence.reviewed_by === 'string' && evidence.reviewed_by.trim(), 'Fixed distribution obligations and reviewer required');
  const inventory = need(evidence.runtime_files, 'Fixed runtime_files required');
  const actual = files(runtime, '', false);
  need(Array.isArray(inventory) && inventory.length === actual.length && new Set(inventory.map(item => item.path)).size === actual.length, 'Fixed license runtime inventory mismatch');
  for (const name of actual) {
    const entry = inventory.find(item => item.path === name);
    need(entry && sha(fs.readFileSync(inside(runtime, name))) === entry.sha256?.toLowerCase(), `Fixed license runtime hash mismatch: ${name}`);
  }
  need(Array.isArray(evidence.notices) && evidence.notices.length, 'Fixed original license/third-party notice files required');
  const texts = evidence.notices.map(entry => {
    need(microsoft(entry.source), 'Fixed notice requires authoritative Microsoft HTTPS source');
    const bytes = fs.readFileSync(inside(path.dirname(evidenceFile), entry.path));
    need(bytes.toString('utf8').trim() && sha(bytes) === entry.sha256?.toLowerCase(), `Fixed notice SHA256 mismatch: ${entry.path}`);
    return { name: entry.path, bytes };
  });
  texts.push({ name: 'REVIEWED_LICENSE_MANIFEST.json', bytes: fs.readFileSync(evidenceFile) });
  return { ecosystem: 'runtime', name: 'Microsoft-WebView2-Fixed', version: evidence.version, scope: 'shipped', source: evidence.source, license: 'Microsoft supplied terms; see original notices and reviewed obligations', notices: texts };
}
export async function collect({ ui, bundle, variant = 'System', runtime, evidence, evidenceSha256, metadata }) {
  need(['System', 'Fixed'].includes(variant), 'variant must be System or Fixed');
  const components = npmPackages(ui);
  if (!metadata) {
    metadata = JSON.parse(execFileSync('cargo', ['metadata', '--locked', '--offline', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc', '--manifest-path', path.join(ui, 'src-tauri/Cargo.toml')], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 }));
  }
  components.push(...rustPackages(metadata));
  if (variant === 'Fixed') components.push(fixedEvidence(need(runtime, 'Fixed runtime required'), evidence, evidenceSha256));
  // Validate all inputs before touching output; replacing our own subtree prevents stale notices.
  const destination = path.join(bundle, 'third_party/ui');
  const pending = destination + '.pending';
  fs.rmSync(pending, { recursive: true, force: true });
  fs.mkdirSync(pending, { recursive: true });
  try {
    const inventory = components.map((component, index) => {
      const { notices: texts, ...fields } = component;
      const copied = texts.sort((a, b) => a.name.localeCompare(b.name)).map(item => {
        const name = `${component.ecosystem}/${index}/${item.name}`;
        const output = inside(pending, name);
        fs.mkdirSync(path.dirname(output), { recursive: true }); fs.writeFileSync(output, item.bytes);
        return { path: `third_party/ui/${name}`, sha256: sha(item.bytes) };
      });
      return { ...fields, notices: copied };
    });
    const document = { schema: 'pubg-highlight-trim/ui-licenses/v1', variant: variant.toLowerCase(), policy: 'Conservative npm production closure; Cargo Windows normal/build closure, no dev-only dependencies. Not a legal approval.',
      locks: { npm: sha(fs.readFileSync(path.join(ui, 'package-lock.json'))), rust: sha(fs.readFileSync(path.join(ui, 'src-tauri/Cargo.lock'))) }, components: inventory };
    fs.writeFileSync(path.join(pending, 'UI_LICENSE_INVENTORY.json'), JSON.stringify(document, null, 2) + '\n');
    fs.rmSync(destination, { recursive: true, force: true }); fs.renameSync(pending, destination);
    return document;
  } finally { fs.rmSync(pending, { recursive: true, force: true }); }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const options = {};
  const names = { '--ui': 'ui', '--bundle': 'bundle', '--variant': 'variant', '--runtime': 'runtime', '--fixed-license-manifest': 'evidence', '--fixed-license-sha256': 'evidenceSha256' };
  try {
    for (let i = 2; i < process.argv.length; i += 2) options[need(names[process.argv[i]], `unknown argument ${process.argv[i]}`)] = need(process.argv[i + 1], 'argument value required');
    need(options.ui && options.bundle, '--ui and --bundle required');
    const result = await collect(options); console.log(`Collected UI notices: ${result.components.length} components`);
  } catch (error) { console.error(`UI license collection failed: ${error.message}`); process.exitCode = 1; }
}
