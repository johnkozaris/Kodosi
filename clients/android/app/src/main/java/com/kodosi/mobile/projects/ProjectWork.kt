package com.kodosi.mobile.projects

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.kodosi.mobile.AppModel
import com.kodosi.mobile.R
import com.kodosi.mobile.runtime.fields
import com.kodosi.mobile.ui.AppIcon
import com.kodosi.mobile.ui.AppSymbol
import com.kodosi.mobile.ui.CheckRow
import com.kodosi.mobile.ui.ConfirmDialog
import com.kodosi.mobile.ui.EmptyState
import com.kodosi.mobile.ui.ErrorNote
import com.kodosi.mobile.ui.KodosiButton
import com.kodosi.mobile.ui.KodosiCard
import com.kodosi.mobile.ui.KodosiDialog
import com.kodosi.mobile.ui.KodosiField
import com.kodosi.mobile.ui.KodosiTextButton
import com.kodosi.mobile.ui.KodosiTheme
import com.kodosi.mobile.ui.KodosiType
import com.kodosi.mobile.ui.ListLine
import com.kodosi.mobile.ui.PersonAvatar
import com.kodosi.mobile.ui.ProgressCaption
import com.kodosi.mobile.ui.SectionTitle
import com.kodosi.mobile.ui.SelectableWords
import com.kodosi.mobile.ui.TextEntryDialog
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@Composable
fun ProjectTasks(model: AppModel, projectId: String, snapshot: ProjectSnapshot?) {
    val scope = rememberCoroutineScope()
    var creating by remember { mutableStateOf(false) }
    var selected by remember { mutableStateOf<String?>(null) }
    var closing by remember { mutableStateOf<ProjectTask?>(null) }
    val tasks = snapshot?.tasks.orEmpty()
    LazyColumn(Modifier.fillMaxSize().padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        item { KodosiTextButton(onClick = { creating = true }) { Text(stringResource(R.string.new_task)) } }
        if (tasks.isEmpty()) item { EmptyState(stringResource(R.string.no_tasks), symbol = AppSymbol.TASKS) }
        for (closed in listOf(false, true)) {
            if (closed && tasks.any { it.closed }) item { SectionTitle(stringResource(R.string.closed_tasks)) }
            items(tasks.filter { it.closed == closed }, key = { it.id }) { task ->
                KodosiCard(onClick = { selected = task.id }) {
                    Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(task.title, style = KodosiType.headline)
                        task.assignedName?.let {
                            Text(it, style = KodosiType.footnote, color = KodosiTheme.colors.inkMuted)
                        }
                        Row {
                            if (!task.closed) {
                                KodosiTextButton(onClick = {
                                    scope.launch {
                                        model.runAction {
                                            model.projects.updateTask(
                                                projectId,
                                                task,
                                                if (task.assignedTo ==
                                                    null
                                                ) {
                                                    "claim"
                                                } else {
                                                    "release"
                                                }
                                            )
                                        }
                                    }
                                }) {
                                    Text(
                                        stringResource(
                                            if (task.assignedTo ==
                                                null
                                            ) {
                                                R.string.claim
                                            } else {
                                                R.string.release
                                            }
                                        )
                                    )
                                }
                            }
                            KodosiTextButton(onClick = {
                                if (task.closed) {
                                    scope.launch {
                                        model.runAction { model.projects.updateTask(projectId, task, "reopen") }
                                    }
                                } else {
                                    closing =
                                        task
                                }
                            }) { Text(stringResource(if (task.closed) R.string.reopen else R.string.close)) }
                        }
                    }
                }
            }
        }
    }
    if (creating) NewTaskDialog(model, projectId, snapshot?.repositories.orEmpty(), { creating = false })
    val task = tasks.firstOrNull { it.id == selected }
    if (task != null) {
        KodosiDialog(
            onDismissRequest = { selected = null },
            title = { Text(task.title) },
            text = {
                Column(
                    Modifier.verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    SelectableWords(task.description)
                    task.note?.let {
                        SectionTitle(stringResource(R.string.result))
                        SelectableWords(it)
                    }
                    task.issue?.let { issue ->
                        val links = com.kodosi.mobile.ui.LocalLinkOpener.current
                        KodosiTextButton(onClick = {
                            links.open(issue.url)
                        }) { Text(stringResource(R.string.open_issue)) }
                    }
                }
            },
            confirmButton = { KodosiTextButton(onClick = { selected = null }) { Text(stringResource(R.string.close)) } }
        )
    }
    closing?.let { closingTask ->
        TextEntryDialog(
            stringResource(R.string.close),
            stringResource(R.string.result),
            initial = closingTask.note.orEmpty(),
            action = stringResource(R.string.close),
            allowBlank = true,
            dismiss = { closing = null }
        ) { model.projects.updateTask(projectId, closingTask, "close", it) }
    }
}

@Composable
private fun NewTaskDialog(
    model: AppModel,
    projectId: String,
    repositories: List<ProjectRepository>,
    dismiss: () -> Unit
) {
    var title by remember { mutableStateOf("") }
    var description by remember { mutableStateOf("") }
    var chosen by remember { mutableStateOf<Set<String>>(emptySet()) }
    var issue by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    KodosiDialog(
        onDismissRequest = { if (!busy) dismiss() },
        title = { Text(stringResource(R.string.new_task)) },
        text = {
            LazyColumn(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                item {
                    KodosiField(title, {
                        title = it
                    }, label = stringResource(R.string.name), singleLine = true)
                }
                item {
                    KodosiField(description, {
                        description = it
                    }, label = stringResource(R.string.description), minLines = 3)
                }
                items(repositories, key = { it.id }) { repository ->
                    CheckRow(repository.name, repository.id in chosen, {
                        chosen = if (it) chosen + repository.id else chosen - repository.id
                    }, enabled = !busy, leading = { AppIcon(AppSymbol.REPOSITORY) })
                }
                if (chosen.isNotEmpty()) {
                    item {
                        CheckRow(stringResource(R.string.create_issue), issue, { issue = it }, enabled = !busy)
                    }
                }
                error?.let { item { ErrorNote(it) } }
            }
        },
        confirmButton = {
            KodosiButton(enabled = !busy && title.isNotBlank(), onClick = {
                busy = true
                scope.launch {
                    try {
                        model.projects.createTask(
                            projectId,
                            title,
                            description,
                            chosen,
                            chosen.sorted().firstOrNull().takeIf {
                                issue
                            }
                        )
                        dismiss()
                    } catch (cancelled: CancellationException) {
                        throw cancelled
                    } catch (failure: Exception) {
                        error = failure.localizedMessage
                    } finally {
                        busy = false
                    }
                }
            }) { Text(stringResource(R.string.create)) }
        },
        dismissButton = {
            KodosiTextButton(enabled = !busy, onClick = dismiss) { Text(stringResource(R.string.cancel)) }
        }
    )
}

@Composable
fun ProjectRepositories(model: AppModel, projectId: String, snapshot: ProjectSnapshot?) {
    var adding by remember { mutableStateOf(false) }
    var selected by remember { mutableStateOf<ProjectRepository?>(null) }
    val links = com.kodosi.mobile.ui.LocalLinkOpener.current
    LazyColumn(Modifier.fillMaxSize().padding(horizontal = 16.dp)) {
        item { KodosiTextButton(onClick = { adding = true }) { Text(stringResource(R.string.add_repository)) } }
        if (snapshot?.repositories.isNullOrEmpty()) {
            item {
                EmptyState(stringResource(R.string.no_repositories), symbol = AppSymbol.REPOSITORY)
            }
        }
        items(snapshot?.repositories.orEmpty(), key = { it.id }) { repository ->
            ListLine(repository.name, subtitle = repository.url) {
                KodosiTextButton(onClick = { selected = repository }) { Text(stringResource(R.string.import_issue)) }
                KodosiTextButton(onClick = { links.open(repository.url) }) { Text(stringResource(R.string.open_page)) }
            }
        }
    }
    if (adding) {
        TextEntryDialog(
            stringResource(R.string.add_repository),
            stringResource(R.string.repository_address),
            action = stringResource(R.string.add_repository),
            dismiss = { adding = false }
        ) { model.projects.addRepository(projectId, it) }
    }
    selected?.let { repository -> IssueDialog(model, projectId, repository, { selected = null }) }
}

@Composable
private fun IssueDialog(model: AppModel, projectId: String, repository: ProjectRepository, dismiss: () -> Unit) {
    var issues by remember { mutableStateOf<List<ProjectIssue>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var importing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(repository.id) {
        try {
            issues = model.projects.issues(projectId, repository.id)
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (failure: Exception) {
            error = failure.localizedMessage
        }
    }
    KodosiDialog(
        onDismissRequest = dismiss,
        title = { Text(repository.name) },
        text = {
            LazyColumn {
                if (issues == null && error == null) item { ProgressCaption(stringResource(R.string.loading)) }
                error?.let { item { ErrorNote(it) } }
                if (issues?.isEmpty() == true) item { Text(stringResource(R.string.no_issues)) }
                items(issues.orEmpty(), key = { it.number }) { issue ->
                    KodosiTextButton(enabled = !importing, onClick = {
                        importing = true
                        scope.launch {
                            try {
                                model.projects.importIssue(projectId, issue)
                                dismiss()
                            } catch (cancelled: CancellationException) {
                                throw cancelled
                            } catch (failure: Exception) {
                                error = failure.localizedMessage
                            } finally {
                                importing = false
                            }
                        }
                    }) { Text("#${issue.number} ${issue.title}") }
                }
            }
        },
        confirmButton = { KodosiTextButton(onClick = dismiss) { Text(stringResource(R.string.close)) } }
    )
}

@Composable
fun ProjectInfo(model: AppModel, projectId: String, left: () -> Unit) {
    val state by model.projects.state.collectAsStateWithLifecycle()
    val account by model.account.state.collectAsStateWithLifecycle()
    val people by model.people.state.collectAsStateWithLifecycle()
    val detail = state.details[projectId]
    val project = model.projects.project(projectId)
    val owner = project?.ownerUserId == account.userId
    var renaming by remember { mutableStateOf(false) }
    var confirming by remember { mutableStateOf<String?>(null) }
    var removing by remember { mutableStateOf<ProjectMember?>(null) }
    val scope = rememberCoroutineScope()
    LazyColumn(Modifier.fillMaxSize().padding(horizontal = 16.dp)) {
        if (owner) item { KodosiTextButton(onClick = { renaming = true }) { Text(stringResource(R.string.rename)) } }
        item { SectionTitle(stringResource(R.string.members)) }
        items(detail?.members.orEmpty(), key = { it.userId }) { member ->
            ListLine(
                member.name,
                subtitle = if (member.isOwner) {
                    stringResource(
                        R.string.owner
                    )
                } else {
                    "@${member.handle}"
                },
                leading = {
                    PersonAvatar(member.name, member.userId)
                }
            ) {
                if (owner &&
                    !member.isOwner
                ) {
                    KodosiTextButton(onClick = { removing = member }) { Text(stringResource(R.string.remove)) }
                }
            }
        }
        item { SectionTitle(stringResource(R.string.invite_friends)) }
        items(
            people.friends.filter { friend ->
                detail?.members?.none { it.userId == friend.userId } == true
            },
            key = { "friend-${it.userId}" }
        ) { friend ->
            ListLine(friend.name, leading = { PersonAvatar(friend.name, friend.userId) }) {
                KodosiTextButton(onClick = {
                    scope.launch {
                        model.runAction {
                            model.commands.request(
                                "project.invite",
                                fields(
                                    "projectId" to projectId,
                                    "userId" to friend.userId
                                )
                            )
                            model.projects.open(projectId)
                        }
                    }
                }) { Text(stringResource(R.string.invite)) }
            }
        }
        item {
            KodosiTextButton(onClick = { confirming = if (owner) "project.delete" else "project.leave" }) {
                Text(
                    stringResource(if (owner) R.string.delete_project else R.string.leave_project),
                    color = KodosiTheme.colors.danger
                )
            }
        }
    }
    if (renaming) {
        TextEntryDialog(
            stringResource(R.string.rename),
            stringResource(R.string.name),
            project?.name.orEmpty(),
            dismiss = {
                renaming =
                    false
            }
        ) {
            model.commands.request("project.rename", fields("projectId" to projectId, "name" to it.trim()))
            model.projects.open(projectId)
        }
    }
    confirming?.let { operation ->
        ConfirmDialog(
            stringResource(if (owner) R.string.delete_project else R.string.leave_project),
            stringResource(if (owner) R.string.delete_project_message else R.string.leave_project_message),
            stringResource(if (owner) R.string.delete else R.string.leave_project),
            { confirming = null },
            {
                scope.launch {
                    model.runAction {
                        model.commands.request(operation, fields("projectId" to projectId))
                        confirming = null
                        left()
                    }
                }
            },
            destructive = true
        )
    }
    removing?.let { member ->
        ConfirmDialog(stringResource(R.string.remove), member.name, stringResource(R.string.remove), {
            removing = null
        }, {
            scope.launch {
                model.runAction {
                    model.commands.request(
                        "project.removeMember",
                        fields(
                            "projectId" to projectId,
                            "userId" to member.userId
                        )
                    )
                    model.projects.open(projectId)
                    removing = null
                }
            }
        }, destructive = true)
    }
}
