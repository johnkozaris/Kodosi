using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;
using Kodosi.Application;
using Microsoft.Extensions.Diagnostics.HealthChecks;

namespace Kodosi.Host.Health;

public sealed class PostQuantumCryptoReadiness(IPopSignatureVerifier verifier)
{
    private const string ResourceName = "Kodosi.Host.Health.MLDsa65KnownAnswer.json";
    private const string SourceCommit = "c58b6f4e0eaf8d3b9792f7add1d3dfcbef40053f";
    private const string SourcePath = "WebCryptoAPI/sign_verify/mldsa_vectors.js";
    private const string SourceExtraction =
        "SubjectPublicKeyInfo BIT STRING payload after the unused-bits octet; OID 2.16.840.1.101.3.4.3.18";
    private const string PublicKeySha256 = "4b473850530cb7ad97ec77e8bd636e8766709c9e94b57934cd3a4ad7d7e5453c";
    private const string MessageSha256 = "9749bfe82c305800cffbcc227d92993221ff5bf9d4027c563bb42f2e4d74064d";
    private const string SignatureSha256 = "4ecb54851060023a9264db5134a98d0e160db1f77b713809af3b6a91dd5df7f4";

    private readonly Lock _gate = new();
    private State _state = State.Pending;
    private string? _failure;

    public void RunOrThrow()
    {
        lock (_gate)
        {
            if (_state == State.Ready)
            {
                return;
            }

            if (_state == State.Failed)
            {
                throw new InvalidOperationException(_failure);
            }

            try
            {
                var vector = LoadVector();
                if (!verifier.Verify(vector.PublicKey, vector.Message, vector.Signature))
                {
                    throw new CryptographicException("The fixed valid ML-DSA-65 signature was rejected.");
                }

                var tamperedMessage = (byte[])vector.Message.Clone();
                tamperedMessage[0] ^= 1;
                if (verifier.Verify(vector.PublicKey, tamperedMessage, vector.Signature))
                {
                    throw new CryptographicException("A modified ML-DSA-65 message was accepted.");
                }

                _state = State.Ready;
            }
            catch (Exception exception)
            {
                _failure = $"ML-DSA-65 startup self-test failed: {exception.Message}";
                _state = State.Failed;
                throw new InvalidOperationException(_failure, exception);
            }
        }
    }

    public HealthCheckResult Health()
    {
        lock (_gate)
        {
            return _state switch
            {
                State.Ready => HealthCheckResult.Healthy(
                    MLDsa.IsSupported
                        ? "ML-DSA-65 fixed-vector verification passed with the native provider."
                        : "ML-DSA-65 fixed-vector verification passed with the BouncyCastle provider."),
                State.Failed => HealthCheckResult.Unhealthy(_failure ?? "ML-DSA-65 startup self-test failed."),
                _ => HealthCheckResult.Unhealthy("ML-DSA-65 startup self-test has not run."),
            };
        }
    }

    private static KnownAnswerVector LoadVector()
    {
        using var stream = typeof(PostQuantumCryptoReadiness).Assembly
            .GetManifestResourceStream(ResourceName)
            ?? throw new InvalidOperationException($"Embedded resource {ResourceName} is missing.");
        var document = JsonSerializer.Deserialize(
            stream,
            KnownAnswerJsonContext.Default.KnownAnswerDocument)
            ?? throw new InvalidOperationException("The embedded known-answer document is empty.");

        ValidateProvenance(document);

        var publicKey = Decode(document.PublicKeyBase64, "public key");
        var message = Decode(document.MessageBase64, "message");
        var signature = Decode(document.SignatureBase64, "signature");
        if (publicKey.Length != 1952 || message.Length != 90 || signature.Length != 3309)
        {
            throw new InvalidOperationException("The embedded known-answer dimensions are invalid.");
        }

        VerifyDigest(publicKey, PublicKeySha256, "public key");
        VerifyDigest(message, MessageSha256, "message");
        VerifyDigest(signature, SignatureSha256, "signature");
        return new KnownAnswerVector(publicKey, message, signature);
    }

    internal static void ValidateProvenance(KnownAnswerDocument document)
    {
        if (document.SchemaVersion != 1
            || document.Algorithm != "ML-DSA-65"
            || document.Context.Length != 0
            || document.Source.Repository != "https://github.com/web-platform-tests/wpt"
            || document.Source.Commit != SourceCommit
            || document.Source.Path != SourcePath
            || document.Source.TestName != "ML-DSA-65 basic test"
            || document.Source.License != "BSD-3-Clause"
            || document.Source.Extraction != SourceExtraction
            || !document.Source.LicenseText.Contains(
                "Copyright © web-platform-tests contributors",
                StringComparison.Ordinal))
        {
            throw new InvalidOperationException("The embedded known-answer provenance is invalid.");
        }
    }

    private static byte[] Decode(string encoded, string label)
    {
        try
        {
            return Convert.FromBase64String(encoded);
        }
        catch (FormatException exception)
        {
            throw new InvalidOperationException($"The embedded known-answer {label} is not valid Base64.", exception);
        }
    }

    private static void VerifyDigest(byte[] value, string expectedHex, string label)
    {
        var expected = Convert.FromHexString(expectedHex);
        var actual = SHA256.HashData(value);
        if (!CryptographicOperations.FixedTimeEquals(actual, expected))
        {
            throw new InvalidOperationException($"The embedded known-answer {label} digest is invalid.");
        }
    }

    private enum State
    {
        Pending,
        Ready,
        Failed,
    }

    private sealed record KnownAnswerVector(byte[] PublicKey, byte[] Message, byte[] Signature);
}

public sealed class PostQuantumCryptoHealthCheck(PostQuantumCryptoReadiness readiness) : IHealthCheck
{
    public Task<HealthCheckResult> CheckHealthAsync(
        HealthCheckContext context,
        CancellationToken cancellationToken = default)
    {
        return Task.FromResult(readiness.Health());
    }
}

internal sealed record KnownAnswerDocument(
    int SchemaVersion,
    string Algorithm,
    string Context,
    KnownAnswerSource Source,
    string PublicKeyBase64,
    string MessageBase64,
    string SignatureBase64);

internal sealed record KnownAnswerSource(
    string Repository,
    string Commit,
    string Path,
    string TestName,
    string License,
    string LicenseText,
    string Extraction);

[JsonSourceGenerationOptions(
    PropertyNamingPolicy = JsonKnownNamingPolicy.CamelCase,
    UnmappedMemberHandling = JsonUnmappedMemberHandling.Disallow)]
[JsonSerializable(typeof(KnownAnswerDocument))]
internal sealed partial class KnownAnswerJsonContext : JsonSerializerContext;
