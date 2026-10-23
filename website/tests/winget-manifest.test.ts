import { describe, expect, test } from 'bun:test';
import { buildManifests, MANIFEST_VERSION, parseDependency, validate, type ReleaseInput } from '../scripts/winget-manifest.ts';

const release: ReleaseInput = {
  id: 'Example.Cleanup',
  version: '1.0.0',
  url: 'https://example.com/releases/cleanup-1.0.0-x64.msix',
  sha256: 'a'.repeat(64),
  publisher: 'cleanup contributors',
};

describe('buildManifests', () => {
  test('writes version, locale, and installer manifests in winget-pkgs layout', () => {
    const files = buildManifests(release);
    expect(files.map((f) => f.path)).toEqual([
      'manifests/e/Example/Cleanup/1.0.0/Example.Cleanup.yaml',
      'manifests/e/Example/Cleanup/1.0.0/Example.Cleanup.locale.en-US.yaml',
      'manifests/e/Example/Cleanup/1.0.0/Example.Cleanup.installer.yaml',
    ]);
    for (const file of files) {
      expect(file.content).toContain(`ManifestVersion: "${MANIFEST_VERSION}"`);
      expect(file.content).toContain('PackageIdentifier: "Example.Cleanup"');
    }
  });

  test('installer manifest describes an x64 MSIX with an uppercase hash', () => {
    const installer = buildManifests({ ...release, dependencies: [parseDependency('Microsoft.WindowsAppRuntime.2@2.5.1')] })[2]!.content;
    expect(installer).toContain('InstallerType: "msix"');
    expect(installer).toContain(`Installers:\n- Architecture: "x64"\n  InstallerUrl: "${release.url}"\n  InstallerSha256: "${'A'.repeat(64)}"`);
    expect(installer).toContain('Dependencies:\n  PackageDependencies:\n  - PackageIdentifier: "Microsoft.WindowsAppRuntime.2"\n    MinimumVersion: "2.5.1"');
    expect(installer).not.toContain('PackageFamilyName');
  });

  test('installer manifest describes the portable EXE with a command alias', () => {
    const installer = buildManifests({ ...release, url: 'https://example.com/releases/download/v1.0.0/Cleanup.exe' })[2]!.content;
    expect(installer).toContain('InstallerType: "portable"');
    expect(installer).toContain('Commands:\n- "cleanup"');
    expect(installer).not.toContain('Dependencies');
    expect(installer).not.toContain('Scope');
  });

  test('round-trips through a YAML parser', () => {
    for (const file of buildManifests({ ...release, packageFamilyName: 'Cleanup_abc123', description: 'Says "hi": yes' })) {
      const parsed = Bun.YAML.parse(file.content) as Record<string, unknown>;
      expect(parsed.PackageVersion).toBe('1.0.0');
    }
  });

  test('refuses incomplete or unsafe releases', () => {
    expect(() => buildManifests({ ...release, url: 'http://example.com/cleanup.msix' })).toThrow('https');
    expect(validate({ ...release, url: 'https://example.com/cleanup.zip' })).toContain('--url must point to the .msix package or portable .exe.');
    expect(validate({ ...release, url: 'https://example.com/Cleanup.exe', packageFamilyName: 'Cleanup_abc123' }))
      .toContain('--package-family-name and --dependency apply only to MSIX packages.');
    expect(validate({ ...release, id: 'cleanup', version: 'v1', sha256: 'nope', publisher: ' ' })).toHaveLength(4);
  });
});
