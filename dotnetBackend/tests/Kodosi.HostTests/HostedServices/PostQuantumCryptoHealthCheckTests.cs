using System.Security.Cryptography;
using Kodosi.Application;
using Kodosi.Host.Health;
using Kodosi.Infrastructure.Crypto;
using Microsoft.Extensions.Diagnostics.HealthChecks;

namespace Kodosi.HostTests;

public sealed class PostQuantumCryptoHealthCheckTests
{
    [Fact]
    public async Task Health_Is_Unhealthy_Until_The_Startup_Self_Test_Passes()
    {
        var verifier = new SequenceVerifier(true, false);
        var readiness = new PostQuantumCryptoReadiness(verifier);
        var check = new PostQuantumCryptoHealthCheck(readiness);

        var pending = await check.CheckHealthAsync(
            new HealthCheckContext(),
            TestContext.Current.CancellationToken);
        Assert.Equal(HealthStatus.Unhealthy, pending.Status);
        Assert.Contains("has not run", pending.Description, StringComparison.Ordinal);

        readiness.RunOrThrow();
        readiness.RunOrThrow();

        var ready = await check.CheckHealthAsync(
            new HealthCheckContext(),
            TestContext.Current.CancellationToken);
        Assert.Equal(HealthStatus.Healthy, ready.Status);
        Assert.Equal(2, verifier.Calls);
    }

    [Fact]
    public async Task Rejected_Known_Answer_Fails_Startup_And_Readiness()
    {
        var readiness = new PostQuantumCryptoReadiness(new SequenceVerifier(false));
        var exception = Assert.Throws<InvalidOperationException>(readiness.RunOrThrow);
        Assert.Contains("fixed valid ML-DSA-65 signature was rejected", exception.Message, StringComparison.Ordinal);

        var result = await new PostQuantumCryptoHealthCheck(readiness).CheckHealthAsync(
            new HealthCheckContext(),
            TestContext.Current.CancellationToken);
        Assert.Equal(HealthStatus.Unhealthy, result.Status);
        Assert.Contains("startup self-test failed", result.Description, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Verifier_Exception_Fails_Startup_And_Readiness()
    {
        var readiness = new PostQuantumCryptoReadiness(new ThrowingVerifier());
        var exception = Assert.Throws<InvalidOperationException>(readiness.RunOrThrow);
        Assert.Contains("provider unavailable", exception.Message, StringComparison.Ordinal);

        var result = await new PostQuantumCryptoHealthCheck(readiness).CheckHealthAsync(
            new HealthCheckContext(),
            TestContext.Current.CancellationToken);
        Assert.Equal(HealthStatus.Unhealthy, result.Status);
        Assert.Contains("provider unavailable", result.Description, StringComparison.Ordinal);
    }

    [Fact]
    public void Provenance_Rejects_A_Different_Public_Key_Extraction()
    {
        var source = new KnownAnswerSource(
            "https://github.com/web-platform-tests/wpt",
            "c58b6f4e0eaf8d3b9792f7add1d3dfcbef40053f",
            "WebCryptoAPI/sign_verify/mldsa_vectors.js",
            "ML-DSA-65 basic test",
            "BSD-3-Clause",
            "Copyright © web-platform-tests contributors",
            "unvalidated alternate extraction");
        var document = new KnownAnswerDocument(1, "ML-DSA-65", "", source, "", "", "");

        var exception = Assert.Throws<InvalidOperationException>(
            () => PostQuantumCryptoReadiness.ValidateProvenance(document));

        Assert.Equal("The embedded known-answer provenance is invalid.", exception.Message);
    }

    [Fact]
    public async Task Production_Verifier_Passes_The_Embedded_Fixed_Vector()
    {
        var readiness = new PostQuantumCryptoReadiness(new MLDsaPopSignatureVerifier());

        readiness.RunOrThrow();

        var result = await new PostQuantumCryptoHealthCheck(readiness).CheckHealthAsync(
            new HealthCheckContext(),
            TestContext.Current.CancellationToken);
        Assert.Equal(HealthStatus.Healthy, result.Status);
    }

    private sealed class SequenceVerifier(params bool[] results) : IPopSignatureVerifier
    {
        private readonly Queue<bool> _results = new(results);

        public int Calls { get; private set; }

        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature)
        {
            Calls++;
            Assert.Equal(1952, publicKey.Length);
            Assert.Equal(90, message.Length);
            Assert.Equal(3309, signature.Length);
            return _results.Dequeue();
        }
    }

    private sealed class ThrowingVerifier : IPopSignatureVerifier
    {
        public bool Verify(
            ReadOnlySpan<byte> publicKey,
            ReadOnlySpan<byte> message,
            ReadOnlySpan<byte> signature)
        {
            throw new CryptographicException("provider unavailable");
        }
    }
}
