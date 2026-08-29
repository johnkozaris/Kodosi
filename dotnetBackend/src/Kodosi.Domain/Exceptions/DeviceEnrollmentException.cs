namespace Kodosi.Domain;

public sealed class DeviceEnrollmentException(string message) : DomainException(message, "DEVICE_ENROLLMENT_INVALID")
{
}
