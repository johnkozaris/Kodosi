using System.Security.Cryptography;
using System.Text.Json;

namespace Kodosi.Push;

public sealed class PushSettings
{
    private PushSettings(ApnsSettings? apns, FcmSettings? fcm)
    {
        Apns = apns; Fcm = fcm;
    }

    internal ApnsSettings? Apns { get; }
    internal FcmSettings? Fcm { get; }
    public bool Sends => Apns is not null || Fcm is not null;

    internal bool Serves(string platform) => platform switch
    {
        "apns" => Apns is not null,
        "fcm" => Fcm is not null,
        _ => false,
    };

    public static PushSettings From(IConfiguration config) =>
        new(ApnsSettings.From(config.GetSection("Push:Apns")), FcmSettings.From(config.GetSection("Push:Fcm")));

    internal static string? Value(IConfigurationSection section, string name) =>
        section[name]?.Trim() is { Length: > 0 } value ? value : null;
}

internal sealed record ApnsSettings(string TeamId, string KeyId, ECDsa Key, string Topic)
{
    public static ApnsSettings? From(IConfigurationSection section)
    {
        string? Value(string name) => PushSettings.Value(section, name);
        var (team, key, pem, topic) = (Value("TeamId"), Value("KeyId"), Value("PrivateKey"), Value("Topic"));
        if (team is null && key is null && pem is null && topic is null) return null;
        if (team is null || key is null || pem is null || topic is null)
            throw new InvalidOperationException("Push:Apns needs TeamId, KeyId, PrivateKey and Topic.");
        if (!team.All(char.IsAsciiLetterOrDigit) || !key.All(char.IsAsciiLetterOrDigit))
            throw new InvalidOperationException("Push:Apns:TeamId and Push:Apns:KeyId are the letters and digits that Apple gives.");
        if (!topic.All(c => char.IsAsciiLetterOrDigit(c) || c is '.' or '-'))
            throw new InvalidOperationException("Push:Apns:Topic must be the bundle ID of the iPhone and iPad app.");
        var signer = ECDsa.Create();
        try
        {
            signer.ImportFromPem(pem);
            if (signer.KeySize != 256) throw new CryptographicException("The key is not a P-256 key.");
        }
        catch (Exception error) when (error is ArgumentException or CryptographicException)
        {
            signer.Dispose();
            throw new InvalidOperationException("Push:Apns:PrivateKey must be the text of the .p8 key file.", error);
        }
        return new ApnsSettings(team, key, signer, topic);
    }
}

internal sealed record FcmSettings(string ProjectId, string ClientEmail, string? KeyId, RSA Key, Uri TokenUri)
{
    private static readonly Uri GoogleTokens = new("https://oauth2.googleapis.com/token");

    public static FcmSettings? From(IConfigurationSection section)
    {
        var (project, account) = (PushSettings.Value(section, "ProjectId"), PushSettings.Value(section, "ServiceAccount"));
        if (project is null && account is null) return null;
        if (project is null || account is null) throw new InvalidOperationException("Push:Fcm needs ProjectId and ServiceAccount.");
        if (!project.All(c => char.IsAsciiLetterLower(c) || char.IsAsciiDigit(c) || c is '-' or '.' or ':'))
            throw new InvalidOperationException("Push:Fcm:ProjectId must be the ID of the Firebase project.");
        string? email, keyId, pem, tokenUri;
        try
        {
            using var json = JsonDocument.Parse(account);
            string? Text(string name) =>
                json.RootElement.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.String ? value.GetString() : null;
            (email, keyId, pem, tokenUri) = (Text("client_email"), Text("private_key_id"), Text("private_key"), Text("token_uri"));
        }
        catch (Exception error) when (error is JsonException or InvalidOperationException)
        {
            throw new InvalidOperationException("Push:Fcm:ServiceAccount must be the JSON text of a Google service account.", error);
        }
        if (string.IsNullOrEmpty(email) || string.IsNullOrEmpty(pem))
            throw new InvalidOperationException("Push:Fcm:ServiceAccount needs client_email and private_key.");
        var tokens = tokenUri is null ? GoogleTokens : Uri.TryCreate(tokenUri, UriKind.Absolute, out var parsed) ? parsed : null;
        if (tokens?.Scheme != Uri.UriSchemeHttps)
            throw new InvalidOperationException("The token_uri of Push:Fcm:ServiceAccount must be an HTTPS address.");
        var signer = RSA.Create();
        try
        {
            signer.ImportFromPem(pem);
        }
        catch (Exception error) when (error is ArgumentException or CryptographicException)
        {
            signer.Dispose();
            throw new InvalidOperationException("The private_key of Push:Fcm:ServiceAccount must be an RSA key.", error);
        }
        return new FcmSettings(project, email, string.IsNullOrEmpty(keyId) ? null : keyId, signer, tokens);
    }
}
