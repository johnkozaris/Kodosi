using System.Buffers.Binary;
using System.Security.Cryptography;
using System.Text;
using Kodosi.Security;
using Org.BouncyCastle.Crypto;
using Org.BouncyCastle.Crypto.Generators;
using Org.BouncyCastle.Crypto.Parameters;
using Org.BouncyCastle.Crypto.Signers;
using Org.BouncyCastle.Security;

namespace Kodosi.HostTests;

internal sealed class DeviceFixture
{
    private readonly AsymmetricCipherKeyPair pair;
    public DeviceFixture(Guid userId, string deviceId)
    {
        UserId = userId; DeviceId = deviceId;
        var generator = new MLDsaKeyPairGenerator();
        generator.Init(new MLDsaKeyGenerationParameters(new SecureRandom(), MLDsaParameters.ml_dsa_65));
        pair = generator.GenerateKeyPair();
        SigningKey = ((MLDsaPublicKeyParameters)pair.Public).GetEncoded();
        KemKey = RandomNumberGenerator.GetBytes(1184);
    }
    public Guid UserId { get; }
    public string DeviceId { get; }
    public byte[] SigningKey { get; }
    public byte[] KemKey { get; }
    public byte[] Sign(ReadOnlySpan<byte> body)
    {
        var signer = new MLDsaSigner(MLDsaParameters.ml_dsa_65, deterministic: false);
        signer.Init(true, pair.Private); signer.BlockUpdate(body); return signer.GenerateSignature();
    }
    public byte[] CertificateBody(string signer, long issued, long expires = 0)
    {
        using var stream = new MemoryStream();
        foreach (var value in new[] { UserId.ToString("D"), DeviceId, "Test device", signer }) Text(stream, value);
        Bytes(stream, KemKey); Bytes(stream, SigningKey); U64(stream, (ulong)issued); U64(stream, (ulong)expires); return stream.ToArray();
    }
    public static byte[] ListBody(Guid user, long generation, IReadOnlyList<(string Device, string Signer)> entries, string signer, long issued, long expires = 0)
    {
        using var stream = new MemoryStream(); Text(stream, user.ToString("D")); U64(stream, (ulong)generation); U32(stream, (uint)entries.Count);
        foreach (var entry in entries) { Text(stream, entry.Device); Text(stream, entry.Signer); }
        Text(stream, signer); U64(stream, (ulong)issued); U64(stream, (ulong)expires); return stream.ToArray();
    }
    private static void Text(Stream stream, string value) => Bytes(stream, Encoding.UTF8.GetBytes(value));
    private static void Bytes(Stream stream, ReadOnlySpan<byte> bytes) { U32(stream, (uint)bytes.Length); stream.Write(bytes); }
    private static void U32(Stream stream, uint value) { Span<byte> bytes = stackalloc byte[4]; BinaryPrimitives.WriteUInt32BigEndian(bytes, value); stream.Write(bytes); }
    private static void U64(Stream stream, ulong value) { Span<byte> bytes = stackalloc byte[8]; BinaryPrimitives.WriteUInt64BigEndian(bytes, value); stream.Write(bytes); }
    public string SignedCertificate(byte[] body) => Convert.ToBase64String(Sign(Proofs.Tagged(DomainTags.DeviceCertV2, body)));
    public string SignedList(byte[] body) => Convert.ToBase64String(Sign(Proofs.Tagged(DomainTags.DeviceListV1, body)));
}
