namespace Kodosi.Domain;

public sealed class UserDeviceList
{
    private byte[] _body = [];
    private byte[] _signature = [];

    public UserId UserId { get; private set; }
    public long Generation { get; private set; }
    public byte[] Body
    {
        get => [.. _body];
        private set => _body = [.. value];
    }
    public byte[] Signature
    {
        get => [.. _signature];
        private set => _signature = [.. value];
    }
    public DateTimeOffset CreatedAt { get; private set; }

    private UserDeviceList() { }

    public SignedDeviceListParser.ParsedSignedDeviceList ParseBody()
    {
        RequireProofIntegrity();
        try
        {
            var parsed = new SignedDeviceListParser().Parse(_body);
            if (parsed.Generation != Generation)
            {
                throw Corrupt("body generation does not match the persistence key");
            }
            if (parsed.UserId != UserId.Value.ToString("D"))
            {
                throw Corrupt("body user_id does not match the persistence owner");
            }
            return parsed;
        }
        catch (SignedDeviceListFormatException exception)
        {
            throw Corrupt(exception.Message, exception);
        }
    }

    public bool MatchesEnrollment(
        UserId userId,
        ReadOnlySpan<byte> body,
        ReadOnlySpan<byte> signature) =>
        UserId == userId
        && _body.AsSpan().SequenceEqual(body)
        && _signature.AsSpan().SequenceEqual(signature);

    public static UserDeviceList Create(
        UserId userId,
        long generation,
        byte[] body,
        byte[] signature)
    {
        if (body is not { Length: > 0 }
            || body.Length > IdentityWireFormat.MaxSignedDeviceListBodyLength)
        {
            throw new DomainException("Device list body length is invalid.");
        }
        if (signature is not { Length: IdentityWireFormat.MlDsa65SignatureLength })
            throw new DomainException("Device list signature length is invalid.");

        SignedDeviceListParser.ParsedSignedDeviceList parsed;
        try
        {
            parsed = new SignedDeviceListParser().Parse(body);
        }
        catch (SignedDeviceListFormatException exception)
        {
            throw new DomainException(
                $"Invalid device list body: {exception.Message}",
                innerException: exception);
        }
        if (parsed.Generation != generation)
            throw new DomainException("Device list body generation must match its persistence key.");
        if (parsed.UserId != userId.Value.ToString("D"))
            throw new DomainException("Device list body user_id must match its owner.");

        return new UserDeviceList
        {
            UserId = userId,
            Generation = generation,
            Body = body,
            Signature = signature,
            CreatedAt = DateTimeOffset.UtcNow,
        };
    }

    private void RequireProofIntegrity()
    {
        if (_body is not { Length: > 0 }
            || _body.Length > IdentityWireFormat.MaxSignedDeviceListBodyLength)
        {
            throw Corrupt("body length is invalid");
        }
        if (_signature.Length != IdentityWireFormat.MlDsa65SignatureLength)
        {
            throw Corrupt("signature length is invalid");
        }
    }

    private static DeviceListCorruptionException Corrupt(
        string detail,
        Exception? innerException = null) =>
        new($"Persisted device list is corrupt: {detail}.", innerException);
}

public sealed class DeviceListCorruptionException : DomainException
{
    public DeviceListCorruptionException(string message, Exception? innerException = null)
        : base(message, "DEVICE_LIST_CORRUPT", innerException)
    {
    }
}
