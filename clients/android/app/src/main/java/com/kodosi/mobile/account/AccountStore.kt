package com.kodosi.mobile.account

import com.kodosi.mobile.runtime.CommandSink
import com.kodosi.mobile.runtime.RuntimeEvent
import com.kodosi.mobile.runtime.RuntimeJson
import com.kodosi.mobile.runtime.Words
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.JsonPrimitive

@Serializable
data class Device(val deviceId: String, val label: String, val recoveryKey: Boolean = false)

data class AccountState(
    val userId: String? = null,
    val displayName: String? = null,
    val authResolved: Boolean = false,
    val localDeviceEnrolled: Boolean = false,
    val selfDeviceId: String? = null,
    val stage: SignInStage = SignInStage.Idle,
    val signInPage: String? = null,
    val devices: List<Device> = emptyList(),
    val onlineHosts: Set<String> = emptySet(),
    val restoring: String? = null,
    val online: Boolean = true
) {
    val isReady: Boolean get() = userId != null && localDeviceEnrolled
    val needsOnboarding: Boolean get() = authResolved && !isReady
}

/** The account of this device: sign-in, the approval of this device and the person's devices. */
class AccountStore(private val commands: CommandSink, private val words: Words, private val language: () -> String) {
    private val mutableState = MutableStateFlow(AccountState())
    val state: StateFlow<AccountState> = mutableState
    var notify: (String) -> Unit = {}

    fun reset(userId: String?) {
        mutableState.update { current ->
            AccountState(userId = userId, stage = if (current.stage.isPending) current.stage else SignInStage.Idle)
        }
    }

    fun receiveAuth(event: RuntimeEvent) {
        if (event.type != "auth.restoring" && event.type != "auth.deletion_pending") {
            mutableState.update { it.copy(restoring = null) }
        }
        when (event.type) {
            "auth.ready" -> receiveReady(event)

            "auth.required" -> receiveRequired(event)

            "auth.restoring" -> mutableState.update { it.copy(restoring = event.string("reason")) }

            "auth.browser" -> {
                val page = event.string("url")?.let(ExternalAuthUrl::parse)
                mutableState.update { it.copy(stage = SignInStage.AwaitingBrowser(page), signInPage = page) }
            }

            "auth.finalizing" -> mutableState.update { it.copy(stage = SignInStage.Finalizing, signInPage = null) }
        }
    }

    fun receiveDevices(event: RuntimeEvent) {
        when {
            event.type == "devices.list" -> receiveList(event)

            event.type == "devices.hosts" -> {
                val ids = event["deviceIds"]?.let {
                    RuntimeJson.decodeFromJsonElement(ListSerializer(String.serializer()), it)
                }
                mutableState.update { it.copy(onlineHosts = ids.orEmpty().toSet()) }
            }

            event.type == "devices.result" && event.string("operation") == "devices.link.startSelf" ->
                mutableState.update {
                    it.copy(
                        stage = SignInStage.TrustingDevice(DeviceTrust.PendingApproval(event.string("code").orEmpty()))
                    )
                }

            event.type == "devices.link.selfResolved" -> {
                mutableState.update {
                    if (it.stage is SignInStage.TrustingDevice && event.string("outcome") != "approved") {
                        it.copy(stage = SignInStage.TrustingDevice(DeviceTrust.Choose))
                    } else {
                        it
                    }
                }
                commands.perform("devices.refresh")
            }

            event.type == "devices.link.resolved" -> commands.perform("devices.refresh")
        }
    }

    /** Follows whether Kodosi reaches the server; a short drop keeps it online. */
    fun receiveConnection(event: RuntimeEvent) {
        if (event.type != "connection.state") return
        val online = event.bool("online") ?: return
        mutableState.update { it.copy(online = online) }
    }

    /** Reaches for the server and the terminals of other computers now. */
    fun retryConnection() {
        commands.perform("connection.retry")
    }

    /** Takes an error that belongs to sign-in; tells whether it is shown already. */
    fun absorb(event: RuntimeEvent, message: String): Boolean {
        val current = mutableState.value
        if (event.type.startsWith("auth.")) {
            if (!current.stage.isPending && current.isReady) return false
            mutableState.update {
                it.copy(stage = SignInStage.Failed(message, it.stage.completedSteps), signInPage = null)
            }
            return current.needsOnboarding || current.userId == null
        }
        if (event.type.startsWith("devices.") && current.stage is SignInStage.TrustingDevice) {
            mutableState.update { it.copy(stage = SignInStage.TrustingDevice(DeviceTrust.Failed(message))) }
            return true
        }
        return false
    }

    fun signIn() {
        val current = mutableState.value
        when (current.stage) {
            SignInStage.Starting,
            is SignInStage.AwaitingBrowser,
            SignInStage.Finalizing,
            is SignInStage.TrustingDevice
            -> Unit

            SignInStage.Idle, is SignInStage.Failed, SignInStage.SignedIn ->
                if (current.userId != null && !current.localDeviceEnrolled) {
                    mutableState.update { it.copy(stage = SignInStage.TrustingDevice(DeviceTrust.Choose)) }
                } else {
                    restartSignIn()
                }
        }
    }

    fun restartSignIn() {
        mutableState.update { it.copy(stage = SignInStage.Starting) }
        commands.perform(
            "auth.login.start",
            mapOf(
                "browser" to JsonPrimitive("app"),
                "language" to JsonPrimitive(language())
            )
        )
    }

    /** The system browser shows the sign-in page now. */
    fun openedSignInPage() {
        mutableState.update { it.copy(signInPage = null) }
    }

    fun finishSignIn(answer: String) {
        if (mutableState.value.stage !is SignInStage.AwaitingBrowser) return
        mutableState.update { it.copy(signInPage = null) }
        commands.perform("auth.login.finish", mapOf("url" to JsonPrimitive(answer)))
    }

    fun cancelSignIn() {
        when (mutableState.value.stage) {
            SignInStage.Starting, is SignInStage.AwaitingBrowser, SignInStage.Finalizing -> {
                commands.perform("auth.login.cancel")
                mutableState.update { it.copy(stage = SignInStage.Idle, signInPage = null) }
            }

            else -> Unit
        }
    }

    fun failSignIn(message: String) {
        commands.perform("auth.login.cancel")
        mutableState.update { it.copy(stage = SignInStage.Failed(message, 0), signInPage = null) }
    }

    fun requestDeviceApproval() {
        mutableState.update { it.copy(stage = SignInStage.TrustingDevice(DeviceTrust.Requesting)) }
        commands.perform("devices.link.startSelf")
    }

    fun cancelDeviceApproval() {
        commands.perform("devices.link.cancelSelf")
        mutableState.update { it.copy(stage = SignInStage.TrustingDevice(DeviceTrust.Choose)) }
    }

    fun useRecoveryKey(key: String) {
        val trimmed = key.trim()
        if (trimmed.isEmpty()) return
        mutableState.update { it.copy(stage = SignInStage.TrustingDevice(DeviceTrust.Recovering)) }
        commands.perform("devices.recovery.use", mapOf("key" to JsonPrimitive(trimmed)))
    }

    fun startFresh() {
        mutableState.update { it.copy(stage = SignInStage.TrustingDevice(DeviceTrust.Resetting)) }
        commands.perform("devices.reset")
    }

    fun chooseTrust() {
        mutableState.update { it.copy(stage = SignInStage.TrustingDevice(DeviceTrust.Choose)) }
    }

    private fun receiveList(event: RuntimeEvent) {
        val enrolled = event.bool("localDeviceEnrolled") ?: false
        val devices = event["devices"]?.let {
            RuntimeJson.decodeFromJsonElement(ListSerializer(Device.serializer()), it)
        }
        mutableState.update {
            it.copy(
                selfDeviceId = event.string("selfDeviceId"),
                localDeviceEnrolled = enrolled,
                devices = devices.orEmpty(),
                stage = if (enrolled && it.stage is SignInStage.TrustingDevice) SignInStage.SignedIn else it.stage
            )
        }
    }

    private fun receiveReady(event: RuntimeEvent) {
        if (event.string("userId") != mutableState.value.userId) {
            notify(words.otherAccount)
            return
        }
        val enrolled = event.bool("enrolled") != false
        mutableState.update {
            it.copy(
                authResolved = true,
                signInPage = null,
                displayName = event.string("displayName") ?: it.displayName,
                localDeviceEnrolled = enrolled,
                stage = when {
                    enrolled -> SignInStage.SignedIn
                    it.stage is SignInStage.TrustingDevice -> it.stage
                    else -> SignInStage.TrustingDevice(DeviceTrust.Choose)
                }
            )
        }
    }

    private fun receiveRequired(event: RuntimeEvent) {
        mutableState.update {
            it.copy(
                authResolved = true,
                signInPage = null,
                stage = if (it.stage is SignInStage.Failed) it.stage else SignInStage.Idle
            )
        }
        when (event.string("reason")) {
            "expired" -> notify(words.signInEnded)
            "accountDeleted" -> notify(words.accountDeleted)
        }
    }
}
