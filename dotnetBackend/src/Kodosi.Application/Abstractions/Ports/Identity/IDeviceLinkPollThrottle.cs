namespace Kodosi.Application;

public interface IDeviceLinkPollThrottle
{
    TimeSpan? TryClaim(string deviceCode);
    void Sweep();
}
