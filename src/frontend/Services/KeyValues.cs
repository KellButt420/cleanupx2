using System.Text;

namespace Cleanup.Services;

/// <summary>
/// Reads Valve's KeyValues text, the format of Steam's libraryfolders.vdf and appmanifest_*.acf files. A value is
/// either a string or a nested dictionary. Keys are case-insensitive.
/// </summary>
internal static class KeyValues
{
    private const int MaxDepth = 64;
    private const int MaxCharacters = 1024 * 1024;
    public static Dictionary<string, object> Parse(string text)
    {
        if (text.Length > MaxCharacters) throw new FormatException("Steam metadata is too large.");
        int position = 0;
        return ParseObject(text, ref position, 0);
    }

    private enum Kind { Text, Open, Close, End }

    private static Dictionary<string, object> ParseObject(string text, ref int position, int depth)
    {
        if (depth >= MaxDepth) throw new FormatException("Steam metadata is nested too deeply.");
        var result = new Dictionary<string, object>(StringComparer.OrdinalIgnoreCase);
        while (true)
        {
            var (kind, key) = Next(text, ref position);
            if (kind is Kind.End or Kind.Close) return result;
            if (kind != Kind.Text) continue;

            var (valueKind, value) = Next(text, ref position);
            if (valueKind == Kind.Text) result[key] = value;
            else if (valueKind == Kind.Open) result[key] = ParseObject(text, ref position, depth + 1);
            else return result;
        }
    }

    private static (Kind Kind, string Text) Next(string text, ref int position)
    {
        while (position < text.Length)
        {
            char current = text[position];
            if (char.IsWhiteSpace(current))
            {
                position++;
            }
            else if (current == '/' && position + 1 < text.Length && text[position + 1] == '/')
            {
                while (position < text.Length && text[position] != '\n') position++;
            }
            else
            {
                break;
            }
        }
        if (position >= text.Length) return (Kind.End, "");

        char start = text[position];
        if (start == '{') { position++; return (Kind.Open, ""); }
        if (start == '}') { position++; return (Kind.Close, ""); }

        if (start == '"')
        {
            // Quoted text: \\ and \" are escapes, and Steam writes Windows paths with doubled backslashes.
            var builder = new StringBuilder();
            position++;
            while (position < text.Length && text[position] != '"')
            {
                if (text[position] == '\\' && position + 1 < text.Length)
                {
                    position++;
                    builder.Append(text[position] switch { 'n' => '\n', 't' => '\t', _ => text[position] });
                }
                else
                {
                    builder.Append(text[position]);
                }
                position++;
            }
            if (position >= text.Length) throw new FormatException("Unterminated Steam metadata string.");
            position++; // The closing quote.
            return (Kind.Text, builder.ToString());
        }

        int began = position;
        while (position < text.Length && !char.IsWhiteSpace(text[position]) && text[position] is not ('{' or '}' or '"')) position++;
        return (Kind.Text, text[began..position]);
    }
}
