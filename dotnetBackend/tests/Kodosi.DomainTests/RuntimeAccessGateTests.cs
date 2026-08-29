using Kodosi.Domain;

namespace Kodosi.DomainTests;

public class RuntimeAccessGateTests
{
    [Theory]
    [InlineData(AccessLevel.Inject, SessionCapability.SendInput, true, SessionStatus.Ended)]
    [InlineData(AccessLevel.Inject, SessionCapability.SendInput, false, SessionStatus.Live)]
    [InlineData(AccessLevel.Suggest, SessionCapability.Suggest, false, SessionStatus.Live)]
    [InlineData(AccessLevel.Suggest, SessionCapability.Suggest, true, SessionStatus.Reconnecting)]
    public void Allows_Returns_False_For_Disallowed_States(
        AccessLevel level,
        SessionCapability capability,
        bool ownerPresent,
        SessionStatus status)
    {
        Assert.False(RuntimeAccessGate.Allows(
            SessionCapabilities.FromAccess(level),
            capability,
            ownerPresent,
            status));
    }

    [Fact]
    public void Inject_Can_Send_Input_When_Owner_Present()
    {
        var result = RuntimeAccessGate.Allows(
            SessionCapabilities.FromAccess(AccessLevel.Inject),
            SessionCapability.SendInput,
            ownerPresent: true,
            SessionStatus.Live);

        Assert.True(result);
    }

    [Fact]
    public void Inject_Does_Not_Advertise_Owner_Only_Resize()
    {
        var capabilities = SessionCapabilities.FromAccess(AccessLevel.Inject);

        Assert.True(capabilities.Allows(SessionCapability.SendInput));
        Assert.True(capabilities.Allows(SessionCapability.Focus));
        Assert.False(capabilities.Allows(SessionCapability.Resize));
    }

    [Fact]
    public void View_Remains_Available_While_Reconnecting()
    {
        var result = RuntimeAccessGate.Allows(
            SessionCapabilities.FromAccess(AccessLevel.View),
            SessionCapability.View,
            ownerPresent: false,
            SessionStatus.Reconnecting);

        Assert.True(result);
    }

    [Fact]
    public void Approver_Can_Decide_But_Cannot_Inject()
    {
        var capabilities = SessionCapabilities.FromAccess(AccessLevel.Approve);

        Assert.True(RuntimeAccessGate.Allows(
            capabilities,
            SessionCapability.ApproveDeny,
            ownerPresent: true,
            SessionStatus.Live));
        Assert.False(RuntimeAccessGate.Allows(
            capabilities,
            SessionCapability.SendInput,
            ownerPresent: true,
            SessionStatus.Live));
    }
}
