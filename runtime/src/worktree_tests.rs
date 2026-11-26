use super::*;

async fn repository() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().expect("temp folder");
    let folder = root.path().join("Project");
    tokio::fs::create_dir(&folder)
        .await
        .expect("repository folder");
    for arguments in [
        vec!["init", "--quiet", "--initial-branch=main"],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "First",
        ],
    ] {
        git(&folder, &arguments).await.expect("git setup");
    }
    let folder = kodosi_os::fs::canonicalize(&folder).expect("canonical");
    (root, folder)
}

#[tokio::test]
async fn a_new_branch_gets_its_own_folder_beside_the_repository() {
    let (_root, folder) = repository().await;
    let inside = folder.join("src");
    tokio::fs::create_dir(&inside).await.expect("subfolder");
    assert!(is_repository(&inside));

    let worktree = create(&inside, "fix/login", None).await.expect("worktree");
    assert_eq!(
        worktree.path,
        folder.parent().expect("parent").join("Project-fix-login")
    );
    assert!(is_repository(&worktree.path));
    assert_eq!(
        git(&worktree.path, &["branch", "--show-current"])
            .await
            .expect("branch"),
        "fix/login"
    );
    assert_eq!(
        create(&folder, "fix/login", None)
            .await
            .expect_err("folder in use")
            .to_string(),
        "The folder Project-fix-login is there already. Use another branch name."
    );
    assert_eq!(
        create(&folder, "main", None)
            .await
            .expect_err("branch in use")
            .to_string(),
        "The branch main is there already. Use another name."
    );
    let refused = create(&folder, "x.lock", None)
        .await
        .expect_err("a name that Git refuses")
        .to_string();
    assert!(
        refused.contains("x.lock") && !refused.starts_with("hint"),
        "{refused}"
    );

    git(
        &worktree.path,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "Work",
        ],
    )
    .await
    .expect("commit");
    let second = create(&worktree.path, "second", None)
        .await
        .expect("second");
    assert_eq!(
        second.path,
        folder.parent().expect("parent").join("Project-second")
    );
    assert_eq!(
        git(&second.path, &["rev-parse", "HEAD"])
            .await
            .expect("head"),
        git(&worktree.path, &["rev-parse", "HEAD"])
            .await
            .expect("head")
    );
    remove_unchanged(second.clone()).await;
    assert!(!second.path.exists());
    assert!(
        git(&folder, &["rev-parse", "--verify", "--quiet", "second"])
            .await
            .is_err()
    );
    assert!(create(&folder, "-bad", None).await.is_err());
    assert!(create(&folder, "a..b", None).await.is_err());
}

#[tokio::test]
async fn an_unchanged_branch_goes_away_and_a_branch_with_work_stays() {
    let (root, folder) = repository().await;
    let place = root.path().join("elsewhere");
    tokio::fs::create_dir(&place).await.expect("place");

    let unchanged = create(&folder, "trial", Some(&place)).await.expect("trial");
    assert!(
        unchanged
            .path
            .starts_with(kodosi_os::fs::canonicalize(&place).expect("place"))
    );
    remove_unchanged(unchanged.clone()).await;
    assert!(!unchanged.path.exists());
    assert!(
        git(&folder, &["rev-parse", "--verify", "--quiet", "trial"])
            .await
            .is_err()
    );

    let edited = create(&folder, "edited", None).await.expect("edited");
    tokio::fs::write(edited.path.join("note.txt"), "work")
        .await
        .expect("file");
    remove_unchanged(edited.clone()).await;
    assert!(edited.path.join("note.txt").exists());

    tokio::fs::write(folder.join(".git/info/exclude"), "*.log\n")
        .await
        .expect("exclude");
    let ignored = create(&folder, "ignored", None).await.expect("ignored");
    tokio::fs::write(ignored.path.join("build.log"), "output")
        .await
        .expect("file");
    remove_unchanged(ignored.clone()).await;
    assert!(ignored.path.join("build.log").exists());

    let committed = create(&folder, "committed", None).await.expect("committed");
    git(
        &committed.path,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "Work",
        ],
    )
    .await
    .expect("commit");
    remove_unchanged(committed.clone()).await;
    assert!(committed.path.exists());

    let switched = create(&folder, "switched", None).await.expect("switched");
    git(&switched.path, &["checkout", "--quiet", "-b", "other"])
        .await
        .expect("other branch");
    git(
        &switched.path,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "Work",
        ],
    )
    .await
    .expect("commit");
    remove_unchanged(switched.clone()).await;
    assert!(switched.path.exists());
}

#[tokio::test]
async fn a_branch_counts_its_commits_edits_and_new_files_but_not_links() {
    let (_root, folder) = repository().await;
    tokio::fs::write(folder.join("app.py"), "one\ntwo\nthree\n")
        .await
        .expect("file");
    let commit = |directory: PathBuf, message: &'static str| async move {
        git(&directory, &["add", "--all"]).await.expect("add");
        git(
            &directory,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--quiet",
                "-m",
                message,
            ],
        )
        .await
        .expect("commit");
    };
    commit(folder.clone(), "App").await;
    let branch = create(&folder, "fix", None).await.expect("worktree");
    assert_eq!(branch.changes().await.expect("count"), None);

    tokio::fs::write(branch.path.join("app.py"), "one\n2\nthree\nfour\n")
        .await
        .expect("edit");
    commit(branch.path.clone(), "Fix").await;
    tokio::fs::write(branch.path.join("app.py"), "one\n2\nthree\nfour\nfive\n")
        .await
        .expect("edit");
    tokio::fs::write(branch.path.join("test_app.py"), "assert True\nassert 1\n")
        .await
        .expect("new file");
    #[cfg(unix)]
    std::os::unix::fs::symlink("/etc/hosts", branch.path.join("hosts")).expect("link");

    assert_eq!(
        branch.changes().await.expect("count"),
        Some(WorktreeChanges {
            added: 5,
            removed: 1
        })
    );
    let diff = branch.diff().await.expect("diff");
    assert!(diff.contains("+five"));
    assert!(diff.contains("+++ b/test_app.py\n@@ -0,0 +1,2 @@\n+assert True"));
    assert!(!diff.contains("hosts"));
}
