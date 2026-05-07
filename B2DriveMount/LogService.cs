using System.IO;

namespace B2DriveMount;

public class LogService
{
    private static readonly string LogDir = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "CloudDriveMount", "logs");

    private static readonly string LogFile = Path.Combine(LogDir, "app.log");
    private static readonly string OldLogFile = Path.Combine(LogDir, "app.old.log");
    private const long MaxLogSizeBytes = 5 * 1024 * 1024; // 5 MB

    private static readonly object Lock = new();

    public static void Write(string level, string message)
    {
        var line = $"[{DateTime.Now:yyyy-MM-dd HH:mm:ss}] [{level}] {message}";
        
        lock (Lock)
        {
            try
            {
                Directory.CreateDirectory(LogDir);

                // Rotate if too big
                if (File.Exists(LogFile))
                {
                    var info = new FileInfo(LogFile);
                    if (info.Length > MaxLogSizeBytes)
                    {
                        if (File.Exists(OldLogFile))
                            File.Delete(OldLogFile);
                        File.Move(LogFile, OldLogFile);
                    }
                }

                File.AppendAllText(LogFile, line + Environment.NewLine);
            }
            catch
            {
                // Logging should never crash the app
            }
        }
    }

    public static void Info(string message) => Write("INFO", message);
    public static void Error(string message) => Write("ERROR", message);
    public static void Debug(string message) => Write("DEBUG", message);

    public static string GetLogDirectory() => LogDir;
    public static string GetLogFilePath() => LogFile;
}
