package com.kodosi.mobile

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.LocalActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.ExitTransition
import androidx.compose.animation.core.FiniteAnimationSpec
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.consumeWindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.adaptive.currentWindowAdaptiveInfoV2
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteScaffoldLayout
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteType
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
import androidx.navigation3.scene.Scene
import androidx.navigation3.ui.NavDisplay
import androidx.window.core.layout.WindowSizeClass
import com.kodosi.mobile.canvas.CanvasScreen
import com.kodosi.mobile.dosi.DosiScreen
import com.kodosi.mobile.onboarding.OnboardingScreen
import com.kodosi.mobile.people.FriendScreen
import com.kodosi.mobile.people.PeopleScreen
import com.kodosi.mobile.projects.ProjectScreen
import com.kodosi.mobile.projects.ProjectsScreen
import com.kodosi.mobile.settings.LicensesScreen
import com.kodosi.mobile.settings.SettingsScreen
import com.kodosi.mobile.terminals.NewTerminalRequest
import com.kodosi.mobile.terminals.NewTerminalSheet
import com.kodosi.mobile.terminals.TerminalScreen
import com.kodosi.mobile.terminals.TerminalsScreen
import com.kodosi.mobile.ui.ConfirmDialog
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiNavigation
import com.kodosi.mobile.ui.KodosiNotice
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.LinkActions
import com.kodosi.mobile.ui.LocalLinkOpener
import com.kodosi.mobile.ui.NavigationItem
import com.kodosi.mobile.ui.Wordmark
import com.kodosi.mobile.ui.fade
import com.kodosi.mobile.ui.groundBackdrop
import com.kodosi.mobile.ui.raised
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable

@Composable
fun App(model: AppModel, openSignIn: (String) -> Unit) {
    val colors = KodosiTheme.colors
    val phase by model.phase.collectAsStateWithLifecycle()
    val account by model.account.state.collectAsStateWithLifecycle()
    val notices = remember { SnackbarHostState() }
    LaunchedEffect(model) { model.notices.collect { notices.showSnackbar(it) } }
    val locked by model.lock.locked.collectAsStateWithLifecycle()
    val home = rememberSaveableStateHolder()
    var previousUser by rememberSaveable { mutableStateOf(account.userId) }
    LaunchedEffect(account.userId, account.authResolved) {
        if (account.authResolved && previousUser != account.userId) {
            previousUser?.let(home::removeState)
            previousUser = account.userId
        }
    }
    LinkActions {
        Box(Modifier.fillMaxSize().groundBackdrop(colors)) {
            val stage = when (val current = phase) {
                AppModel.Phase.Launching -> Stage.Launch

                is AppModel.Phase.Failed -> Stage.Failure(current.message)

                AppModel.Phase.Ready -> when {
                    locked -> Stage.Locked

                    account.isReady -> Stage.Home(account.userId.orEmpty())

                    account.restoring != null -> Stage.Failure(
                        stringResource(
                            when (account.restoring) {
                                "offline" -> R.string.account_restore_offline
                                "unreadable" -> R.string.account_restore_unreadable
                                "updateRequired" -> R.string.account_restore_update_required
                                else -> R.string.account_restore_unavailable
                            }
                        ),
                        update = account.restoring == "updateRequired"
                    )

                    account.authResolved -> Stage.Onboarding

                    else -> Stage.Launch
                }
            }
            val fading = fade<Float>()
            AnimatedContent(
                stage,
                Modifier.fillMaxSize(),
                transitionSpec = {
                    val transform = if (initialState is Stage.Home && targetState == Stage.Locked) {
                        EnterTransition.None togetherWith ExitTransition.None
                    } else {
                        fadeIn(fading) togetherWith fadeOut(fading)
                    }
                    transform using null
                },
                contentKey = { it.key },
                label = "stage"
            ) { shown ->
                when (shown) {
                    Stage.Launch -> Launch()

                    is Stage.Failure -> Failure(shown.message, model::retry, update = shown.update)

                    Stage.Locked -> Locked(model)

                    // The app lock hides the person's work in the frame that it locks, not after a fade.
                    is Stage.Home -> if (!locked) home.SaveableStateProvider(shown.userId) { Home(model) }

                    Stage.Onboarding -> OnboardingScreen(model.account, openSignIn)
                }
            }
            SnackbarHost(notices, Modifier.align(Alignment.BottomCenter).navigationBarsPadding()) { KodosiNotice(it) }
        }
    }
}

// Saved state finds its place again after Android ends the app only under keys that hash the same in each process,
// which a class does not.
private sealed class Stage(val key: String) {
    data object Launch : Stage("launch")

    data class Failure(val message: String, val update: Boolean = false) : Stage("failure")

    data object Locked : Stage("locked")

    data class Home(val userId: String) : Stage("home")

    data object Onboarding : Stage("onboarding")
}

@Serializable
private data class Destination(val kind: String, val id: String = "", val part: String? = null) : NavKey

@Composable
private fun Home(model: AppModel) {
    val colors = KodosiTheme.colors
    var tab by rememberSaveable { mutableIntStateOf(0) }
    val stacks = listOf(
        rememberNavBackStack(Destination("terminals")),
        rememberNavBackStack(Destination("projects")),
        rememberNavBackStack(Destination("dosi")),
        rememberNavBackStack(Destination("settings"))
    )
    val stack = stacks[tab]
    val terminals by model.terminals.state.collectAsStateWithLifecycle()
    val visibleTerminal = (stack.lastOrNull() as? Destination)?.takeIf {
        it.kind == "terminal" || it.kind == "canvas"
    }?.id
    val visibleSession = terminals.sessions.firstOrNull { it.id == visibleTerminal }
    LaunchedEffect(visibleTerminal, visibleSession?.canOpen) {
        visibleSession?.takeIf { it.canOpen }?.let { model.terminals.open(it.id) }
    }
    LifecycleResumeEffect(visibleTerminal) {
        visibleTerminal?.let { model.terminals.watch(it, true) }
        onPauseOrDispose { visibleTerminal?.let { model.terminals.watch(it, false) } }
    }
    DisposableEffect(visibleTerminal) {
        onDispose { visibleTerminal?.let(model.terminals::disconnect) }
    }
    val links = LocalLinkOpener.current
    val projects by model.projects.state.collectAsStateWithLifecycle()
    val account by model.account.state.collectAsStateWithLifecycle()
    val people by model.people.state.collectAsStateWithLifecycle()
    val dosi by model.dosi.state.collectAsStateWithLifecycle()
    val preferences by model.preferences.state.collectAsStateWithLifecycle()
    val target by model.terminalTarget.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val notificationPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        model.updatePush(force = true)
    }
    LaunchedEffect(account.isReady) {
        if (account.isReady &&
            model.preferences.state.value.notifications && !model.preferences.notificationPermissionAsked &&
            context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            model.preferences.notificationPermissionAsked()
            notificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }
    var opening by rememberSaveable { mutableStateOf<String?>(null) }
    var openingTab by rememberSaveable { mutableIntStateOf(0) }
    fun showTerminal(id: String, at: Int) {
        val destination = Destination("terminal", id)
        val entries = stacks[at]
        val previous = entries.indexOf(destination)
        if (previous >= 0) {
            while (entries.lastIndex > previous) entries.removeAt(entries.lastIndex)
        } else {
            entries.add(destination)
        }
    }
    var askingNetwork by rememberSaveable { mutableStateOf(false) }
    fun showOpening() {
        opening?.let { id ->
            if (model.terminals.state.value.sessions.any { it.id == id }) {
                showTerminal(id, openingTab)
            }
        }
        opening = null
    }
    val network = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { showOpening() }
    fun openTerminal(id: String, at: Int) {
        // Only a terminal of the person's own computer can go straight on the local network.
        val own = model.terminals.state.value.sessions.any { it.id == id && it.isOwner }
        if (opening != null) {
            opening = id
            openingTab = at
        } else if (own && Build.VERSION.SDK_INT >= Build.VERSION_CODES.CINNAMON_BUN &&
            !model.preferences.localNetworkAsked &&
            context.checkSelfPermission(Manifest.permission.ACCESS_LOCAL_NETWORK) != PackageManager.PERMISSION_GRANTED
        ) {
            model.preferences.localNetworkAsked()
            opening = id
            openingTab = at
            askingNetwork = true
        } else {
            showTerminal(id, at)
        }
    }
    // Android names this permission "nearby devices", which says nothing of a person's own computer.
    if (askingNetwork) {
        ConfirmDialog(
            stringResource(R.string.local_network_title),
            stringResource(R.string.local_network_reason),
            stringResource(R.string.local_network_continue),
            {
                askingNetwork = false
                showOpening()
            },
            {
                askingNetwork = false
                network.launch(Manifest.permission.ACCESS_LOCAL_NETWORK)
            }
        )
    }
    var starting by remember { mutableStateOf<NewTerminalRequest?>(null) }
    val titles = listOf(R.string.terminals, R.string.projects, R.string.dosi, R.string.settings)
    val symbols = listOf(AppSymbol.TERMINAL, AppSymbol.PROJECTS, AppSymbol.DOSI, AppSymbol.SETTINGS)
    val waiting = terminals.sessions.count { !it.hidden && it.needsUser && (it.isOwner || it.canControl) }
    val counts = listOf(
        waiting,
        projects.badge,
        dosi.badge,
        people.incoming.size + account.requests.size + if (account.reset?.thisDevice == false) 1 else 0
    )
    LaunchedEffect(target) {
        val requested = target ?: return@LaunchedEffect
        val shown = model.resolve(requested)
        starting = null
        tab = 0
        while (stacks[0].size > 1) stacks[0].removeAt(stacks[0].lastIndex)
        shown?.let { openTerminal(it.id, 0) }
        if (model.terminalTarget.value == requested) model.terminalTarget.value = null
    }
    val invite by model.friendInvite.collectAsStateWithLifecycle()
    LaunchedEffect(invite != null) {
        if (invite == null) return@LaunchedEffect
        tab = 3
        val settings = stacks[3]
        if ((settings.lastOrNull() as? Destination)?.kind != "friends") {
            while (settings.size > 1) settings.removeAt(settings.lastIndex)
            settings.add(Destination("friends"))
        }
    }
    val back = {
        if (stack.size > 1) stack.removeAt(stack.lastIndex)
        Unit
    }
    val terminal: (String) -> Unit = { openTerminal(it, tab) }
    val fading = fade<Float>()
    val sliding = fade<IntOffset>()
    val forward = if (LocalLayoutDirection.current == LayoutDirection.Rtl) -1 else 1
    val shift = with(LocalDensity.current) { PushShift.roundToPx() } * forward
    val content: @Composable () -> Unit = {
        NavDisplay(
            backStack = stack,
            onBack = back,
            transitionSpec = {
                if (initialState.root() == targetState.root()) {
                    push(shift, fading, sliding)
                } else {
                    fadeIn(fading) togetherWith fadeOut(fading)
                }
            },
            popTransitionSpec = { push(-shift, fading, sliding) },
            predictivePopTransitionSpec = { _ -> push(-shift, fading, sliding) },
            entryProvider = entryProvider {
                entry<Destination> { destination ->
                    when (destination.kind) {
                        "terminals" -> TerminalsScreen(
                            model.terminals,
                            account.computers.map {
                                it.deviceId
                            }.toSet(),
                            account.online,
                            summaryFirst = preferences.summaryFirst,
                            open = terminal,
                            start = {
                                starting = it
                            }
                        )

                        "projects" -> ProjectsScreen(model) { stack.add(Destination("project", it)) }

                        "project" -> ProjectScreen(model, destination.id, back, terminal) {
                            starting =
                                NewTerminalRequest(project = it)
                        }

                        "friends" -> PeopleScreen(model, back) { stack.add(Destination("friend", it)) }

                        "friend" -> FriendScreen(model, destination.id, back, terminal)

                        "dosi" -> DosiScreen(model) { kind, value ->
                            when (kind) {
                                "terminal" -> model.terminals.state.value.sessions.firstOrNull {
                                    it.id == value || it.name == value
                                }?.let { terminal(it.id) }

                                "project" -> projects.projects.firstOrNull { it.id == value || it.name == value }?.let {
                                    stack.add(Destination("project", it.id))
                                }

                                "terminals" -> tab = 0

                                "projects" -> tab = 1

                                "url" -> links.open(value)
                            }
                        }

                        "settings" -> SettingsScreen(
                            model,
                            friends = { stack.add(Destination("friends")) },
                            licenses = { stack.add(Destination("licenses")) }
                        )

                        "licenses" -> LicensesScreen(back)

                        "terminal" -> TerminalScreen(model, destination.id, account.online, back, canvas = { at ->
                            val current = model.terminals.state.value.sessions.firstOrNull { it.id == destination.id }
                            val selected = current?.canvas?.openKey(
                                at,
                                model.preferences.state.value.closedCanvasParts(destination.id, current.canvas)
                            ) ?: at ?: "browser"
                            model.preferences.canvasPart(
                                destination.id,
                                if (selected == "browser") "browser" else "files",
                                true
                            )
                            stack.add(Destination("canvas", destination.id, selected))
                        })

                        "canvas" -> CanvasScreen(model, destination.id, back, ask = back, initialKey = destination.part)
                    }
                }
            }
        )
    }
    val adaptive = currentWindowAdaptiveInfoV2()
    val compact = !adaptive.windowSizeClass.isWidthAtLeastBreakpoint(WindowSizeClass.WIDTH_DP_MEDIUM_LOWER_BOUND)
    val destination = (stack.lastOrNull() as? Destination)?.kind
    val fullScreen = destination in setOf("terminal", "canvas") || (destination == "project" && compact)
    val layoutType = if (fullScreen) {
        NavigationSuiteType.None
    } else if (compact) {
        NavigationSuiteType.NavigationBar
    } else {
        NavigationSuiteType.NavigationRail
    }
    NavigationSuiteScaffoldLayout(
        navigationSuiteType = layoutType,
        navigationSuite = {
            if (!fullScreen) {
                KodosiNavigation(
                    titles.mapIndexed { index, title ->
                        NavigationItem(stringResource(title), symbols[index], counts[index])
                    },
                    tab,
                    rail = !compact
                ) { tab = it }
            }
        }
    ) {
        val pane = if (layoutType == NavigationSuiteType.NavigationRail) {
            val shape = RoundedCornerShape(18.dp)
            Modifier.statusBarsPadding().padding(top = 8.dp, end = 8.dp, bottom = 8.dp)
                .raised(colors, shape, fill = colors.surface).clip(shape)
        } else {
            Modifier.background(colors.surface)
        }
        Box(
            Modifier.fillMaxSize().then(pane).then(
                if (layoutType == NavigationSuiteType.NavigationBar) {
                    Modifier.consumeWindowInsets(WindowInsets.navigationBars.only(WindowInsetsSides.Bottom))
                } else {
                    Modifier.navigationBarsPadding().imePadding()
                }
            )
        ) { content() }
    }
    starting?.let { request ->
        NewTerminalSheet(model, request, dismiss = { starting = null }, started = terminal)
    }
}

private val PushShift = 32.dp

/** A tab's stack starts at its own root, so a scene with another root is another tab. */
private fun Scene<*>.root(): Any? = (previousEntries.firstOrNull() ?: entries.firstOrNull())?.contentKey

/** Going back runs the same path in reverse, so that a screen leaves the way it came. */
private fun push(shift: Int, fading: FiniteAnimationSpec<Float>, sliding: FiniteAnimationSpec<IntOffset>) =
    (fadeIn(fading) + slideInHorizontally(sliding) { shift }) togetherWith
        (fadeOut(fading) + slideOutHorizontally(sliding) { -shift })

@Composable
private fun Locked(model: AppModel) {
    val activity = LocalActivity.current ?: return
    val scope = androidx.compose.runtime.rememberCoroutineScope()
    Column(
        Modifier.fillMaxSize(),
        verticalArrangement = Arrangement.spacedBy(24.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Wordmark(height = 30.dp)
        KodosiButton(stringResource(R.string.unlock), { scope.launch { model.lock.unlock(activity) } })
    }
}

@Composable
private fun Launch() {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Wordmark(height = 30.dp) }
}

@Composable
private fun Failure(message: String, retry: () -> Unit, update: Boolean = false) {
    val colors = KodosiTheme.colors
    val context = LocalContext.current
    val links = LocalLinkOpener.current
    Column(
        verticalArrangement = Arrangement.spacedBy(20.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier.fillMaxSize().padding(32.dp)
    ) {
        Wordmark(height = 24.dp)
        Text(
            message,
            style = KodosiType.callout,
            color = colors.inkMuted,
            textAlign = TextAlign.Center,
            modifier = Modifier.widthIn(max = 440.dp)
        )
        if (update) {
            KodosiButton(stringResource(R.string.update), onClick = { links.open(updatePage(context)) })
            KodosiTextButton(onClick = retry) { Text(stringResource(R.string.try_again)) }
        } else {
            KodosiButton(stringResource(R.string.try_again), onClick = retry)
        }
    }
}

private fun updatePage(context: Context): String {
    val store = runCatching {
        context.packageManager.getInstallSourceInfo(context.packageName).installingPackageName
    }.getOrNull()
    return if (store == "com.android.vending") {
        "https://play.google.com/store/apps/details?id=${context.packageName}"
    } else {
        "https://updates.kodosi.com/download/android"
    }
}
