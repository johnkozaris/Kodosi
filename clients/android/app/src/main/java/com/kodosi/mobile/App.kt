package com.kodosi.mobile

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.LocalActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.material3.adaptive.currentWindowAdaptiveInfo
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation3.runtime.NavKey
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.runtime.rememberNavBackStack
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
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiNavigation
import com.kodosi.mobile.ui.KodosiNotice
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.LinkActions
import com.kodosi.mobile.ui.LocalLinkOpener
import com.kodosi.mobile.ui.NavigationItem
import com.kodosi.mobile.ui.Wordmark
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
            when (val current = phase) {
                AppModel.Phase.Launching -> Launch()

                is AppModel.Phase.Failed -> Failure(current.message, model::retry)

                AppModel.Phase.Ready -> when {
                    locked -> Locked(model)

                    account.isReady -> home.SaveableStateProvider(account.userId.orEmpty()) { Home(model) }

                    account.restoring != null -> Failure(
                        stringResource(
                            when (account.restoring) {
                                "offline" -> R.string.account_restore_offline
                                "unreadable" -> R.string.account_restore_unreadable
                                "updateRequired" -> R.string.account_restore_update_required
                                else -> R.string.account_restore_unavailable
                            }
                        ),
                        model::retry
                    )

                    account.authResolved -> OnboardingScreen(model.account, openSignIn)

                    else -> Launch()
                }
            }
            SnackbarHost(notices, Modifier.align(Alignment.BottomCenter).navigationBarsPadding()) { KodosiNotice(it) }
        }
    }
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
        visibleSession?.takeIf { it.canOpen }?.let { session ->
            model.runAction { model.terminals.open(session.id) }
        }
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
    var networkAsked by rememberSaveable { mutableStateOf(false) }
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
    val network = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {
        opening?.let { id ->
            if (model.terminals.state.value.sessions.any { it.id == id }) {
                showTerminal(id, openingTab)
            }
        }
        opening = null
    }
    fun openTerminal(id: String, at: Int) {
        if (opening != null) {
            opening = id
            openingTab = at
        } else if (Build.VERSION.SDK_INT >= 37 && !networkAsked &&
            context.checkSelfPermission(Manifest.permission.ACCESS_LOCAL_NETWORK) != PackageManager.PERMISSION_GRANTED
        ) {
            networkAsked = true
            opening = id
            openingTab = at
            network.launch(Manifest.permission.ACCESS_LOCAL_NETWORK)
        } else {
            showTerminal(id, at)
        }
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
    val terminalUnavailable = stringResource(R.string.terminal_not_available)
    LaunchedEffect(target) {
        val requested = target ?: return@LaunchedEffect
        model.runAction {
            if (model.resolve(requested)) {
                tab = 0
                openTerminal(requested.id, 0)
            } else if (model.account.state.value.userId == requested.account) {
                model.notify(terminalUnavailable)
            }
        }
        if (model.terminalTarget.value == requested) model.terminalTarget.value = null
    }
    val back = {
        if (stack.size > 1) stack.removeAt(stack.lastIndex)
        Unit
    }
    val terminal: (String) -> Unit = { openTerminal(it, tab) }
    val content: @Composable () -> Unit = {
        NavDisplay(
            backStack = stack,
            onBack = back,
            entryProvider = entryProvider {
                entry<Destination> { destination ->
                    when (destination.kind) {
                        "terminals" -> TerminalsScreen(
                            model.terminals,
                            account.computers.map {
                                it.deviceId
                            }.toSet(),
                            account.online,
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
    val adaptive = currentWindowAdaptiveInfo()
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
private fun Failure(message: String, retry: () -> Unit) {
    val colors = KodosiTheme.colors
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
        KodosiButton(stringResource(R.string.try_again), onClick = retry)
    }
}
