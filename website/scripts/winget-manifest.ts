// Builds the three WinGet manifests (version, default locale, installer) for an
// MSIX package or the portable Cleanup.exe, chosen by the installer URL's
// extension. Pure: takes release facts, returns files. No network, no disk.

export const MANIFEST_VERSION = '1.10.0';

export type Dependency = { id: string; minimumVersion?: string };

export type ReleaseInput = {
  id: string;
  version: string;
  url: string;
  sha256: string;
  publisher: string;
  name?: string;
  license?: string;
  locale?: string;
  shortDescription?: string;
  description?: string;
  publisherUrl?: string;
  packageUrl?: string;
  licenseUrl?: string;
  releaseNotesUrl?: string;
  packageFamilyName?: string;
  signatureSha256?: string;
  minimumOSVersion?: string;
  dependencies?: Dependency[];
};

export type ManifestFile = { path: string; content: string };

const ID = /^[^.\s\\/:*?"<>|]{1,32}(\.[^.\s\\/:*?"<>|]{1,32}){1,7}$/;
const VERSION = /^\d+(\.\d+){0,3}$/;
const SHA256 = /^[0-9A-Fa-f]{64}$/;

const DEFAULTS = {
  name: 'cleanup',
  license: 'MIT',
  locale: 'en-US',
  shortDescription: 'Remove applications and organize folders on Windows 11, with a preview before you act.',
  description:
    'cleanup is a native Windows 11 app. Remove applications through their official uninstallers, ' +
    'review leftovers separately, and organize folders after previewing every move. History lets you undo eligible file moves.',
  minimumOSVersion: '10.0.22000.0',
} as const;

export function validate(input: ReleaseInput): string[] {
  const errors: string[] = [];
  if (!ID.test(input.id)) errors.push(`--id "${input.id}" must look like Publisher.Package.`);
  if (!VERSION.test(input.version)) errors.push(`--version "${input.version}" must be numeric, like 1.0.0.`);
  if (!SHA256.test(input.sha256)) errors.push('--sha256 must be 64 hexadecimal characters.');
  if (input.signatureSha256 && !SHA256.test(input.signatureSha256)) errors.push('--signature-sha256 must be 64 hexadecimal characters.');
  if (!input.publisher.trim()) errors.push('--publisher is required.');
  for (const [flag, value] of [
    ['--url', input.url],
    ['--publisher-url', input.publisherUrl],
    ['--package-url', input.packageUrl],
    ['--license-url', input.licenseUrl],
    ['--release-notes-url', input.releaseNotesUrl],
  ] as const) {
    if (value !== undefined && !isHttps(value)) errors.push(`${flag} must be an https:// URL.`);
  }
  if (!installerType(input.url)) errors.push('--url must point to the .msix package or portable .exe.');
  if (installerType(input.url) === 'portable' && (input.packageFamilyName || input.dependencies?.length)) {
    errors.push('--package-family-name and --dependency apply only to MSIX packages.');
  }
  for (const dependency of input.dependencies ?? []) {
    if (!ID.test(dependency.id)) errors.push(`--dependency "${dependency.id}" must be a WinGet package identifier.`);
  }
  return errors;
}

export function buildManifests(input: ReleaseInput): ManifestFile[] {
  const errors = validate(input);
  if (errors.length) throw new Error(errors.join('\n'));

  const release = { ...DEFAULTS, ...withoutUndefined(input) };
  const [publisherSegment, ...packageSegments] = release.id.split('.');
  const directory = ['manifests', publisherSegment!.charAt(0).toLowerCase(), publisherSegment!, ...packageSegments, release.version].join('/');
  const common = { PackageIdentifier: release.id, PackageVersion: release.version };

  const version = yaml('version', {
    ...common,
    DefaultLocale: release.locale,
    ManifestType: 'version',
    ManifestVersion: MANIFEST_VERSION,
  });

  const locale = yaml('defaultLocale', {
    ...common,
    PackageLocale: release.locale,
    Publisher: release.publisher,
    PublisherUrl: release.publisherUrl,
    PackageName: release.name,
    PackageUrl: release.packageUrl,
    License: release.license,
    LicenseUrl: release.licenseUrl,
    ShortDescription: release.shortDescription,
    Description: release.description,
    Moniker: 'cleanup',
    Tags: ['uninstaller', 'organizer', 'files', 'windows-11'],
    ReleaseNotesUrl: release.releaseNotesUrl,
    ManifestType: 'defaultLocale',
    ManifestVersion: MANIFEST_VERSION,
  });

  const dependencies = release.dependencies?.length
    ? { PackageDependencies: release.dependencies.map((d) => ({ PackageIdentifier: d.id, MinimumVersion: d.minimumVersion })) }
    : undefined;

  const type = installerType(release.url)!;
  const installer = yaml('installer', {
    ...common,
    MinimumOSVersion: release.minimumOSVersion,
    InstallerType: type,
    // Portable installs get a `cleanup` command alias in WinGet's Links folder.
    Commands: type === 'portable' ? ['cleanup'] : undefined,
    Scope: 'user',
    UpgradeBehavior: 'install',
    PackageFamilyName: release.packageFamilyName,
    Dependencies: dependencies,
    Installers: [
      {
        Architecture: 'x64',
        InstallerUrl: release.url,
        InstallerSha256: release.sha256.toUpperCase(),
        SignatureSha256: release.signatureSha256?.toUpperCase(),
      },
    ],
    ManifestType: 'installer',
    ManifestVersion: MANIFEST_VERSION,
  });

  return [
    { path: `${directory}/${release.id}.yaml`, content: version },
    { path: `${directory}/${release.id}.locale.${release.locale}.yaml`, content: locale },
    { path: `${directory}/${release.id}.installer.yaml`, content: installer },
  ];
}

export function parseDependency(value: string): Dependency {
  const [id = '', minimumVersion] = value.split('@');
  return minimumVersion ? { id, minimumVersion } : { id };
}

export function installerType(url: string): 'msix' | 'portable' | null {
  const path = url.toLowerCase().split('?')[0]!;
  return path.endsWith('.msix') ? 'msix' : path.endsWith('.exe') ? 'portable' : null;
}

function isHttps(value: string) {
  try {
    return new URL(value).protocol === 'https:';
  } catch {
    return false;
  }
}

function withoutUndefined<T extends object>(value: T): T {
  return Object.fromEntries(Object.entries(value).filter(([, v]) => v !== undefined)) as T;
}

// Minimal YAML writer for the shapes above. Strings are JSON-quoted, which is valid YAML.
type Value = string | number | boolean | undefined | Value[] | { [key: string]: Value };

function yaml(type: string, document: Record<string, Value>): string {
  const header = `# yaml-language-server: $schema=https://aka.ms/winget-manifest.${type}.${MANIFEST_VERSION}.schema.json\n\n`;
  return header + writeMap(document, 0).join('\n') + '\n';
}

function writeMap(map: Record<string, Value>, depth: number): string[] {
  const pad = '  '.repeat(depth);
  return Object.entries(map).flatMap(([key, value]) => {
    if (value === undefined) return [];
    if (Array.isArray(value)) return [`${pad}${key}:`, ...writeList(value, depth)];
    if (typeof value === 'object') return [`${pad}${key}:`, ...writeMap(value, depth + 1)];
    return [`${pad}${key}: ${scalar(value)}`];
  });
}

function writeList(list: Value[], depth: number): string[] {
  const pad = '  '.repeat(depth);
  return list.flatMap((item) => {
    if (item === undefined) return [];
    if (typeof item === 'object' && !Array.isArray(item)) {
      const [first = '', ...rest] = writeMap(item, depth + 1);
      return [`${pad}- ${first.trimStart()}`, ...rest];
    }
    return [`${pad}- ${scalar(item as string | number | boolean)}`];
  });
}

const scalar = (value: string | number | boolean) => (typeof value === 'string' ? JSON.stringify(value) : String(value));
