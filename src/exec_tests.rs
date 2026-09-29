use super::*;

fn param(name: &str, value: &str, param_type: ParamType) -> Parameter {
    Parameter {
        name: name.to_owned(),
        value: value.to_owned(),
        param_type,
    }
}

#[test]
fn substitutes_braced_names_only() {
    let params = vec![param("ENV", "prod", ParamType::String)];
    assert_eq!(
        substitute("deploy ${ENV} to $ENV", &params, false),
        "deploy prod to $ENV"
    );
}

#[test]
fn leaves_unknown_names_to_the_shell() {
    let params = vec![param("FOO", "1", ParamType::String)];
    assert_eq!(
        substitute("${HOME} ${FOOBAR} ${FOO} ${", &params, false),
        "${HOME} ${FOOBAR} 1 ${"
    );
}

#[test]
fn masks_passwords_only_when_asked() {
    let params = vec![param("PWD", "s3cret", ParamType::Password)];
    assert_eq!(substitute("login ${PWD}", &params, false), "login s3cret");
    assert_eq!(
        substitute("login ${PWD}", &params, true),
        format!("login {MASK}")
    );
}

#[test]
fn estimate_is_median_of_completed_runs() {
    let make = |secs: i64| ExecutionLog {
        id: new_id(),
        command_id: "c".to_owned(),
        script: String::new(),
        output: String::new(),
        exit_code: Some(0),
        start_time: Local::now(),
        end_time: Some(Local::now() + chrono::Duration::seconds(secs)),
        params: Vec::new(),
    };
    let logs = vec![make(2), make(10), make(6)];
    assert_eq!(estimate_secs(&logs), Some(6.0));
    assert_eq!(estimate_secs(&[]), None);
}

#[cfg(unix)]
#[test]
fn cancel_kills_children_the_command_spawned_too() {
    // Реальный случай: `apt update`, `ansible` и подобные заводят себе
    // дочерние процессы. Тут это `sleep`, оставленный висеть в фоне и
    // унаследовавший наши stdout/stderr, — если бы «Остановить» убивало
    // только саму оболочку, чтение вывода никогда не увидело бы конца
    // потока (в дочернем процессе всё ещё открыт пишущий конец пайпа),
    // и запуск навсегда остался бы «выполняется» даже после отмены.
    let mut command = Command::new("Тест", None);
    // "started" печатается уже ПОСЛЕ того, как фоновый sleep реально
    // зафоркан, — ждём её в выводе, чтобы наверняка отменять команду,
    // когда убивать действительно есть кого, а не саму ещё не успевшую
    // запуститься оболочку (иначе тест ничего бы не проверял).
    command.script = "sleep 999 &\necho started\nwait\n".to_owned();
    let mut run = ActiveRun::start(&command, Vec::new(), None);

    let spawn_deadline = Instant::now() + Duration::from_secs(5);
    while !run.output.contains("started") && Instant::now() < spawn_deadline {
        run.poll();
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        run.output.contains("started"),
        "фоновый процесс должен был успеть запуститься"
    );

    run.cancel();

    let cancel_deadline = Instant::now() + Duration::from_secs(5);
    while !run.finished && Instant::now() < cancel_deadline {
        run.poll();
        thread::sleep(Duration::from_millis(20));
    }

    assert!(
        run.finished,
        "запуск должен завершиться после «Остановить», даже если команда \
         оставила в фоне собственного потомка"
    );
}

#[cfg(unix)]
#[test]
fn the_shell_has_no_controlling_terminal() {
    // `setsid()` в shell_command должен сделать оболочку лидером новой
    // сессии — тогда у сессии нет управляющего терминала вовсе, и такие
    // программы, как sudo или ssh, не находят, куда молча написать
    // «Password:» и повиснуть в ожидании ввода, которого никто не даст.
    // Сессия без терминала проявляется как pid сессии, равный pid самой
    // оболочки, — это и проверяем: `ps` печатает их оба.
    let mut command = Command::new("Тест", None);
    command.script = "ps -o pid=,sid= -p $$\n".to_owned();
    let mut run = ActiveRun::start(&command, Vec::new(), None);

    let deadline = Instant::now() + Duration::from_secs(5);
    while !run.finished && Instant::now() < deadline {
        run.poll();
        thread::sleep(Duration::from_millis(10));
    }
    assert!(run.finished, "команда должна была успеть завершиться");

    let numbers: Vec<i64> = run
        .output
        .split_whitespace()
        .filter_map(|word| word.parse().ok())
        .collect();
    let [pid, sid] = numbers[..] else {
        panic!(
            "ожидались pid и sid одной строкой, получили: {:?}",
            run.output
        );
    };
    assert_eq!(
        pid, sid,
        "pid и sid оболочки должны совпадать — иначе она не стала лидером \
         своей сессии и осталась привязана к терминалу cmdrerun; вывод: {:?}",
        run.output
    );
}
