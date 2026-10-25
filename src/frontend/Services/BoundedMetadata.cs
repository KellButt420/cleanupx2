using System.Text;

namespace Cleanup.Services;

internal static class BoundedMetadata
{
    public static byte[] ReadBytes(string path, int maximum = 1024 * 1024)
    {
        using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
        if (stream.Length > maximum) throw new IOException("Metadata exceeds the supported size.");
        byte[] data = new byte[checked((int)stream.Length)];
        stream.ReadExactly(data);
        return data;
    }

    public static string ReadText(string path) => Encoding.UTF8.GetString(ReadBytes(path)).TrimStart('\uFEFF');
}
