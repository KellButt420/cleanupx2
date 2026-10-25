using Cleanup.Services;
using System.Text.Json;

int passed = 0;
void Check(string name, Action test) { test(); Console.WriteLine($"PASS {name}"); passed++; }
void Expect<T>(Action test) where T : Exception { try { test(); } catch (T) { return; } throw new Exception($"Expected {typeof(T).Name}"); }
void Assert(bool value) { if (!value) throw new Exception("Assertion failed"); }
ReleaseVersion V(string value) => ReleaseVersion.Parse(value) ?? throw new Exception(value);

Check("Steam metadata stays compatible", () => Assert(KeyValues.Parse("\"LibraryFolders\" { \"path\" \"D:\\\\games\" }")["libraryfolders"] is Dictionary<string, object>));
Check("deep metadata raises a recoverable exception", () => Expect<FormatException>(() => KeyValues.Parse(string.Concat(Enumerable.Repeat("a {", 50_000)))));
Check("oversized and unterminated metadata is rejected", () => { Expect<FormatException>(() => KeyValues.Parse(new string('a', 1024 * 1024 + 1))); Expect<FormatException>(() => KeyValues.Parse("\"unfinished")); });
Check("semantic release ordering", () => {
    string[] versions = ["1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta", "1.0.0-beta.2", "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0"];
    for (int i = 1; i < versions.Length; i++) Assert(V(versions[i-1]).CompareTo(V(versions[i])) < 0);
    Assert(V("v0.1.5-rc.1").CompareTo(V("v0.1.5-rc.2")) < 0);
    Assert(V("1.2.3+build").CompareTo(V("1.2.3")) == 0);
    Assert(ReleaseVersion.Parse("1.0.0-rc.01") is null);
});
Check("updates respect stable and preview channels", () => {
    using JsonDocument releases = JsonDocument.Parse("""[{"draft":false,"prerelease":true,"tag_name":"v0.1.5-rc.2","html_url":"https://github.com/test/app/releases/tag/v0.1.5-rc.2"},{"draft":false,"prerelease":false,"tag_name":"v0.1.4","html_url":"https://github.com/test/app/releases/tag/v0.1.4"}]""");
    var repo = new Uri("https://github.com/test/app");
    Assert(UpdateCheck.SelectNewer(releases.RootElement, repo, V("0.1.5-rc.1"))?.Tag == "v0.1.5-rc.2");
    Assert(UpdateCheck.SelectNewer(releases.RootElement, repo, V("0.1.3"))?.Tag == "v0.1.4");
    Assert(UpdateCheck.SelectNewer(releases.RootElement, repo, V("0.1.4")) is null);
});
Check("timed out storage work is reused and capped", () => {
    var flights = new SingleFlight<string, int>(1);
    using var release = new ManualResetEventSlim();
    Task<int> first = flights.Run("drive", () => { release.Wait(); return 7; });
    try {
        for (int i = 0; i < 100; i++) Assert(ReferenceEquals(first, flights.Run("drive", () => 9)));
        Expect<IOException>(() => flights.Run("another", () => 9).GetAwaiter().GetResult());
    } finally { release.Set(); first.GetAwaiter().GetResult(); }
    Assert(flights.Run("another", () => 9).GetAwaiter().GetResult() == 9);
});
Check("metadata read limits apply before allocation", () => {
    string root = Path.Combine(AppContext.BaseDirectory, "fixtures"); Directory.CreateDirectory(root);
    string file = Path.Combine(root, "large.dat");
    try { using (var stream = File.Create(file)) stream.SetLength(1024 * 1024 + 1); Expect<IOException>(() => BoundedMetadata.ReadBytes(file)); }
    finally { File.Delete(file); }
});
Console.WriteLine($"{passed} managed regression checks passed.");
