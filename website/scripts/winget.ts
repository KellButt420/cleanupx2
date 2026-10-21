// bun run winget:prepare --id Publisher.Cleanup --version 1.0.0 --url https://…/cleanup.msix --file ./cleanup.msix --publisher "…"
// Writes manifests to winget-output/. Never submits anything.

import { parseArgs } from 'node:util';
import { join } from 'node:path';
import { buildManifests, parseDependency, validate, type ReleaseInput } from './winget-manifest.ts';

const { values } = parseArgs({
  options: {
    id: { type: 'string' },
    version: { type: 'string' },
    url: { type: 'string' },
    file: { type: 'string' },
    sha256: { type: 'string' },
    publisher: { type: 'string' },
    'publisher-url': { type: 'string' },
    'package-url': { type: 'string' },
    'license-url': { type: 'string' },
    'release-notes-url': { type: 'string' },
    'package-family-name': { type: 'string' },
    'signature-sha256': { type: 'string' },
    dependency: { type: 'string', multiple: true },
    out: { type: 'string', default: 'winget-output' },
    help: { type: 'boolean', short: 'h' },
  },
  strict: true,
});

if (values.help) {
  console.log(`Usage: bun run winget:prepare --id <Publisher.Package> --version <x.y.z> --url <https://…/cleanup.msix>
  (--file <local .msix> | --sha256 <hash>) --publisher <name>
  [--publisher-url] [--package-url] [--license-url] [--release-notes-url]
  [--package-family-name] [--signature-sha256] [--dependency <Id[@MinVersion]>]… [--out <dir>]`);
  process.exit(0);
}

async function sha256Of(path: string) {
  const file = Bun.file(path);
  if (!(await file.exists())) throw new Error(`--file not found: ${path}`);
  return new Bun.CryptoHasher('sha256').update(await file.arrayBuffer()).digest('hex').toUpperCase();
}

const fileHash = values.file ? await sha256Of(values.file) : undefined;
if (fileHash && values.sha256 && fileHash !== values.sha256.toUpperCase()) {
  console.error(`--sha256 does not match --file (${fileHash}).`);
  process.exit(1);
}

const input: ReleaseInput = {
  id: values.id ?? '',
  version: values.version ?? '',
  url: values.url ?? '',
  sha256: fileHash ?? values.sha256 ?? '',
  publisher: values.publisher ?? '',
  publisherUrl: values['publisher-url'],
  packageUrl: values['package-url'],
  licenseUrl: values['license-url'],
  releaseNotesUrl: values['release-notes-url'],
  packageFamilyName: values['package-family-name'],
  signatureSha256: values['signature-sha256'],
  dependencies: values.dependency?.map(parseDependency),
};

const errors = validate(input);
if (errors.length) {
  console.error(errors.join('\n'));
  console.error('\nRun with --help for usage.');
  process.exit(1);
}

for (const manifest of buildManifests(input)) {
  const target = join(values.out, manifest.path);
  await Bun.write(target, manifest.content);
  console.log(`wrote ${target}`);
}

if (!input.packageFamilyName) console.warn('note: --package-family-name not set; WinGet matches upgrades more reliably with it.');
console.log('\nNext: winget validate --manifest <folder>, then winget install --manifest <folder> on a clean machine.');
