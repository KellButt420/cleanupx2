using System.Text;

namespace Cleanup.Services;

/// <summary>Reads the title from a PS3 PARAM.SFO file, the small key/value file that describes an installed game.</summary>
internal static class ParamSfo
{
    public static string? ReadTitle(string path)
    {
        try
        {
            byte[] data = File.ReadAllBytes(path);
            if (data.Length < 20 || data[0] != 0 || data[1] != (byte)'P' || data[2] != (byte)'S' || data[3] != (byte)'F')
            {
                return null;
            }
            long keyTable = BitConverter.ToUInt32(data, 8);
            long dataTable = BitConverter.ToUInt32(data, 12);
            uint entries = BitConverter.ToUInt32(data, 16);
            for (int i = 0; i < entries && 20 + i * 16 + 16 <= data.Length; i++)
            {
                int entry = 20 + i * 16;
                if (ReadKey(data, keyTable + BitConverter.ToUInt16(data, entry)) != "TITLE") continue;

                long start = dataTable + BitConverter.ToUInt32(data, entry + 12);
                long length = BitConverter.ToUInt32(data, entry + 4);
                if (start + length > data.Length) return null;

                string title = Encoding.UTF8.GetString(data, (int)start, (int)length).TrimEnd('\0', ' ');
                return title.Length > 0 ? title : null;
            }
        }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or ArgumentException)
        {
            // A damaged file simply has no readable title; the folder name is used instead.
        }
        return null;
    }

    private static string ReadKey(byte[] data, long offset)
    {
        if (offset >= data.Length) return "";
        int end = (int)offset;
        while (end < data.Length && data[end] != 0) end++;
        return Encoding.ASCII.GetString(data, (int)offset, end - (int)offset);
    }
}
