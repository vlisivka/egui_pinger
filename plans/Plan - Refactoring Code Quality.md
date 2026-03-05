# План: Рефакторинг якості коду egui_pinger

## Огляд

Після проведення аудиту якості коду (див. `research.md`) виявлено 15 проблем різного рівня серйозності. Цей план описує покроковий рефакторинг у 5 фазах — від швидких механічних виправлень до структурних змін та покриття тестами. Кожна фаза є самодостатньою: після її завершення проєкт залишається робочим і компілюється.

**Принцип:** кожен крок завершується `cargo test` + `cargo clippy`. Жоден крок не додає нових функцій — тільки покращує структуру, читабельність та тестованість існуючого коду.

---

## Зміни в архітектурі

### Нові файли
| Файл | Призначення |
|------|-------------|
| `src/constants.rs` | Іменовані константи замість магічних чисел |
| `src/ui/host_settings.rs` | Вікно налаштувань хоста (виокремлено з `app.rs`) |
| `src/ui/route_viewer.rs` | Модальне вікно трасування маршруту |
| `src/ui/log_viewer.rs` | Вікно журналу подій |
| `src/ui/help.rs` | Довідка по метрикам |
| `src/ui/host_row.rs` | Рендеринг одного рядка хоста з графіком |
| `tests/pinger_logic_tests.rs` | Тести бізнес-логіки пінгера |

### Файли, які будуть суттєво змінені
| Файл | Поточний розмір | Очікуваний розмір | Що змінюється |
|------|----------------|-------------------|---------------|
| `src/app.rs` | 1236 рядків | ~350–400 рядків | Виокремлення UI-компонентів у `src/ui/` |
| `src/logic/pinger.rs` | 639 рядків | ~200–250 рядків | Декомпозиція `pinger_task()` на функції |
| `src/model/status.rs` | 693 рядки | ~750 рядків (з підструктурами) | Додавання методів, розбиття `HostStatus` |
| `src/logic/tracer.rs` | 144 рядки | ~145 рядків | `LazyLock` для Regex |
| `src/ui/mod.rs` | 1 рядок | ~10 рядків | Реєстрація нових модулів |

### Файли, які НЕ потрібно змінювати
- `src/main.rs` — вже чистий і компактний (57 рядків)
- `src/model/app_state.rs` — простий і стабільний
- `src/model/status_tests.rs` — 44 тести, не переписуємо
- `tests/gui_tests.rs` — 22 тести, не переписуємо

---

## Етапи реалізації

### Фаза 1: Швидкі перемоги (механічні зміни, низький ризик)

Мета: усунути прості технічні борги, що не потребують структурних змін.

#### 1.1 Clippy auto-fix
- [ ] Виконати `cargo clippy --fix --lib -p egui_pinger` для автоматичного виправлення 14 попереджень (7× `collapsible_if`, 1× `unwrap_or_default`, 6× інших).
- [ ] Переглянути diff — переконатися, що auto-fix коректний.
- [ ] `cargo test` — підтвердити, що нічого не зламано.

#### 1.2 Створення модуля констант
- [ ] Створити файл `src/constants.rs` з іменованими константами:
  ```rust
  /// Size of the sliding window for RTT and jitter history.
  pub const HISTORY_WINDOW_SIZE: usize = 300;

  /// Maximum number of events stored per host in memory.
  pub const MAX_EVENTS_PER_HOST: usize = 100_000;

  /// Maximum events displayed in the UI log viewer.
  pub const MAX_UI_EVENTS: usize = 10_000;

  /// RTT threshold (ms) for the warning line on the chart.
  pub const RTT_WARNING_THRESHOLD_MS: f64 = 150.0;

  /// Number of consecutive failures/successes to confirm state change.
  pub const STATE_CONFIRMATION_STREAK: u32 = 3;

  /// Freshness timeout for hop data in failure deduction (seconds).
  pub const HOP_DATA_FRESHNESS_SEC: u64 = 15;

  /// Interval between periodic traceroutes (seconds).
  pub const TRACEROUTE_INTERVAL_SEC: u64 = 3600;

  /// Minimum cooldown before re-tracing after status change (seconds).
  pub const TRACEROUTE_MIN_COOLDOWN_SEC: u64 = 60;

  /// Periodic statistics snapshot interval (every N pings).
  pub const STATS_SNAPSHOT_INTERVAL: u32 = 300;

  /// RFC 3550 smoothing divisor for RTP jitter calculation.
  pub const RTP_JITTER_SMOOTHING_DIVISOR: f64 = 16.0;
  ```
- [ ] Зареєструвати `pub mod constants;` у `src/lib.rs`.
- [ ] Замінити всі магічні числа у `src/model/status.rs`, `src/logic/pinger.rs`, `src/app.rs` на константи.
- [ ] `cargo test` + `cargo clippy` — підтвердити коректність.

#### 1.3 Метод `HostStatus::trim_events()`
- [ ] Додати в `src/model/status.rs`:
  ```rust
  /// Trims the event log to the maximum allowed size.
  pub fn trim_events(&mut self) {
      while self.events.len() > MAX_EVENTS_PER_HOST {
          self.events.pop_front();
      }
  }
  ```
- [ ] Замінити 5 дублювань у `app.rs` (рядки ~205, ~650) та `pinger.rs` (рядки ~136, ~538, ~601) на `status.trim_events()`.
- [ ] `cargo test`.

#### 1.4 Функція `log_entry_color()`
- [ ] Створити функцію у `src/app.rs` (або в окремому утиліт-модулі):
  ```rust
  fn log_entry_color(entry: &LogEntry, visuals: &PingVisuals) -> Color32 {
      match entry {
          LogEntry::Ping { rtt: None, .. } => Color32::from_rgb(230, 159, 0),
          LogEntry::Incident { is_break: true, .. } => Color32::from_rgb(213, 94, 0),
          LogEntry::Incident { is_break: false, .. } => Color32::from_rgb(0, 158, 115),
          LogEntry::Statistics { .. } => Color32::from_rgb(0, 158, 115),
          LogEntry::RouteUpdate { .. } => Color32::from_rgb(0, 114, 178),
          LogEntry::Marker { .. } => Color32::from_rgb(204, 121, 167),
          _ => visuals.latency_color(0.1),
      }
  }
  ```
- [ ] Замінити два дубльованих блоки (рядки ~1114–1134 та ~1179–1198) на виклик `log_entry_color()`.
- [ ] `cargo test`.

#### 1.5 Виправлення коментарів
- [ ] Виправити помилкові коментарі: «last 99» → «last 300» у `status.rs` (рядки 382, 384, 392).
- [ ] Обрати англійську мову для всіх коментарів (відповідно до мовної політики специфікації).
- [ ] Перевести коментарі українською на англійську (пріоритет: `app.rs`, `pinger.rs`).
- [ ] Додати `///` doc-коментарі до структур `HostStatus`, `LogEntry`, `EguiPinger`, `TracerouteHop`.

---

### Фаза 2: Рефакторинг UI — декомпозиція `app.rs`

Мета: розбити «God Function» `ui_layout()` (991 рядків) на окремі методи у відповідних файлах.

#### 2.1 Підготовка інфраструктури `src/ui/`
- [x] Оновити `src/ui/mod.rs` для реєстрації нових модулів:
  ```rust
  pub mod system_tools;
  pub mod host_settings;
  pub mod route_viewer;
  pub mod log_viewer;
  pub mod help;
  pub mod host_row;
  ```
- [x] Визначити, які дані потрібні кожному модулю (передавати `&mut EguiPinger` або набір конкретних параметрів).

#### 2.2 Виокремити вікно налаштувань хоста
- [x] Створити `src/ui/host_settings.rs`.
- [x] Перенести код з `app.rs` рядки ~695–850 (вікно налаштувань) у функцію:
  ```rust
  pub fn render_host_settings_window(
      ctx: &egui::Context,
      visuals: &PingVisuals,
      hosts: &mut Vec<HostInfo>,
      editing_index: &mut Option<usize>,
  );
  ```
- [x] `cargo test`.

#### 2.3 Виокремити вікно трасування маршруту
- [x] Створити `src/ui/route_viewer.rs`.
- [x] Перенести код з `app.rs` рядки ~852–910 у функцію:
  ```rust
  pub fn render_route_window(
      ctx: &egui::Context,
      visuals: &PingVisuals,
      statuses: &HashMap<String, HostStatus>,
      route_host: &mut Option<String>,
  );
  ```
- [x] `cargo test`.

#### 2.4 Виокремити вікно журналу подій
- [x] Створити `src/ui/log_viewer.rs`.
- [x] Перенести код з `app.rs` рядки ~1012–1214 у функцію:
  ```rust
  pub fn render_log_window(
      ctx: &egui::Context,
      visuals: &PingVisuals,
      hosts: &[HostInfo],
      statuses: &HashMap<String, HostStatus>,
      log_host: &mut Option<String>,
      log_filter: &mut LogFilter,
      log_stick_to_bottom: &mut bool,
  );
  ```
- [x] Перемістити `log_entry_color()` (з кроку 1.4) у цей модуль.
- [x] `cargo test`.

#### 2.5 Виокремити довідку по метрикам
- [x] Створити `src/ui/help.rs`.
- [x] Перенести код з `app.rs` рядки ~912–1005 у функцію:
  ```rust
  pub fn render_help_window(
      ctx: &egui::Context,
      help_open: &mut bool,
      help_tab: &mut HelpTab,
  );
  ```
- [x] `cargo test`.

#### 2.6 Виокремити рендеринг рядка хоста
- [x] Створити `src/ui/host_row.rs`.
- [x] Перенести код з `app.rs` рядки ~317–605 (основний список хостів із графіками) у функцію:
  ```rust
  pub fn render_host_row(
      ui: &mut egui::Ui,
      visuals: &PingVisuals,
      host: &HostInfo,
      status: &HostStatus,
      index: usize,
      // ... callbacks або мутабельні посилання на необхідні стани
  );
  ```
- [x] `cargo test`.

#### 2.7 `PingMode::label()`
- [x] Додати метод у `src/model/status.rs`:
  ```rust
  impl PingMode {
      pub fn label(&self) -> String {
          match self {
              PingMode::VeryFast => tr!("Very fast (1s)"),
              PingMode::Fast => tr!("Fast (2s)"),
              PingMode::NotFast => tr!("Not fast (5s)"),
              PingMode::Normal => tr!("Normal (10s)"),
              PingMode::NotSlow => tr!("Not slow (30s)"),
              PingMode::Slow => tr!("Slow (1m)"),
              PingMode::VerySlow => tr!("Very slow (5m)"),
          }
      }
  }
  ```
- [x] Замінити два дубльованих блоки у вікні налаштувань хоста на виклик `h.mode.label()`.
- [x] `cargo test`.

#### 2.8 Валідація декомпозиції
- [x] Переконатися, що `ui_layout()` залишився тонким «оркестратором» (~100–150 рядків): конфігурує top/bottom panels та викликає `render_*` функції.
- [x] `cargo test` + `cargo clippy`.

---

### Фаза 3: Рефакторинг логіки пінгера — декомпозиція `pinger.rs`

Мета: розбити «God Function» `pinger_task()` (589 рядків) на тестовані функції.

#### 3.1 Виокремити логіку traceroute
- [ ] Створити функцію:
  ```rust
  /// Checks which hosts need traceroute updates and spawns traceroute tasks.
  fn check_and_spawn_traceroutes(
      state: &SharedState,
      trace_tasks: &mut HashMap<String, JoinHandle<Vec<String>>>,
      now: Instant,
  );
  ```
- [ ] Перенести код з рядків ~60–160 `pinger_task()`.
- [ ] `cargo test`.

#### 3.2 Виокремити діагностичний режим
- [ ] Створити функцію:
  ```rust
  /// Enables/disables diagnostic (high-frequency) pinging for hops of down hosts.
  fn update_diagnostic_modes(state: &SharedState);
  ```
- [ ] Перенести код з рядків ~162–196.
- [ ] `cargo test`.

#### 3.3 Виокремити планування пінгів
- [ ] Створити функцію:
  ```rust
  /// Collects the list of addresses that need pinging in the current cycle.
  fn collect_ping_targets(state: &SharedState, now: Instant) -> Vec<PingTarget>;
  ```
  де `PingTarget` — невеликий struct з адресою, режимом та метаданими.
- [ ] Перенести код з рядків ~198–268.
- [ ] `cargo test`.

#### 3.4 Виокремити failure deduction
- [ ] Створити функцію:
  ```rust
  /// Analyzes hop-by-hop data to deduce which node caused a connectivity failure.
  fn deduce_failure_points(state: &SharedState);
  ```
- [ ] Перенести код з рядків ~270–388.
- [ ] **Обов'язково:** ця функція має бути публічною (або `pub(crate)`) для тестування у Фазі 4.
- [ ] `cargo test`.

#### 3.5 Виокремити обробку результатів пінгу
- [ ] Створити функцію:
  ```rust
  /// Processes a single ping result: updates stats, creates log entries, detects incidents.
  fn process_ping_result(
      state: &SharedState,
      address: &str,
      alive: bool,
      rtt_ms: f64,
      seq: u32,
      bytes: u16,
  );
  ```
- [ ] Замінити два дубльованих блоки (рядки ~460–561 та ~564–627) єдиною функцією.
- [ ] `cargo test`.

#### 3.6 `HostInfo::append_to_log()`
- [ ] Додати метод до `HostInfo`:
  ```rust
  /// Appends formatted log lines to the host's log file (if logging is enabled).
  pub fn append_to_log(&self, lines: &[String]) {
      if !self.log_to_file || self.log_file_path.is_empty() {
          return;
      }
      if let Ok(mut file) = std::fs::OpenOptions::new()
          .create(true)
          .append(true)
          .open(&self.log_file_path)
      {
          for line in lines {
              let _ = writeln!(file, "{}", line);
          }
      }
  }
  ```
- [ ] Замінити 4 дублювання у `pinger.rs` та `app.rs`.
- [ ] `cargo test`.

#### 3.7 Валідація декомпозиції
- [ ] Переконатися, що `pinger_task()` залишився тонким основним циклом (~100–150 рядків).
- [ ] `cargo test` + `cargo clippy`.

---

### Фаза 4: Тести для бізнес-логіки пінгера

Мета: покрити тестами найкритичніші алгоритми, які раніше були недоступні для тестування.

> **Передумова:** ці тести стають можливими лише після Фази 3, коли логіка виокремлена у тестовані функції.

#### 4.1 Тести failure deduction
- [ ] Створити `tests/pinger_logic_tests.rs` (або `src/logic/pinger_logic_tests.rs`).
- [ ] Написати тести:
  - [ ] Хост DOWN, усі хопи UP → `failure_point = None` (проблема на самому хості).
  - [ ] Хост DOWN, хоп N DOWN → `failure_point = hop[N].address`.
  - [ ] Два хости з однаковим шлюзом, обидва DOWN, шлюз DOWN → `failure_point = gateway`, `dependent_targets` = обидва.
  - [ ] Застарілі дані хопів (>15с) → ігноруються у deduction.
  - [ ] Невідомі хопи (`*`) → не вважаються "broken".
  - [ ] Хост DOWN, перший хоп DOWN → `failure_point = "Local Interface"`.

#### 4.2 Тести incident detection
- [ ] Написати тести:
  - [ ] 3 послідовних timeout → створюється `LogEntry::Incident { is_break: true }`.
  - [ ] Після `is_break: true`, 1 успіх → створюється `LogEntry::Incident { is_break: false }` з `downtime_sec`.
  - [ ] Перше відправлення — не генерує інцидент (ще немає `prev_alive`).

#### 4.3 Тести traceroute update logic
- [ ] Написати тести:
  - [ ] Порожній traceroute НЕ перезаписує валідний шлях.
  - [ ] Новий валідний traceroute замінює старий.
  - [ ] Зміна стану alive→down тригерить re-trace.
  - [ ] Re-trace не тригериться частіше ніж `TRACEROUTE_MIN_COOLDOWN_SEC`.

#### 4.4 Валідація
- [ ] `cargo test` — усі нові тести проходять.
- [ ] Переконатися, що покриття найкритичніших шляхів > 0 (failure deduction, incident detection).

---

### Фаза 5: Технічний борг та фінальне полірування

Мета: усунути залишкові проблеми продуктивності та якості.

#### 5.1 `Vec<f64>` → `VecDeque<f64>` для history
- [ ] Замінити `history: Vec<f64>` на `history: VecDeque<f64>` у `HostStatus`.
- [ ] Замінити `self.history.push(rtt_ms)` + `self.history.remove(0)` на `self.history.push_back(rtt_ms)` + `self.history.pop_front()`.
- [ ] Аналогічно для `rtp_jitter_history`.
- [ ] Оновити `add_sample()`: замінити `self.history.iter()...collect::<Vec>` на пряму ітерацію де можливо.
- [ ] `cargo test`.

#### 5.2 `LazyLock` для Regex у `tracer.rs`
- [ ] Замінити `Regex::new(...)` всередині `parse_traceroute_output()` на:
  ```rust
  use std::sync::LazyLock;
  static IP_RE: LazyLock<Regex> = LazyLock::new(|| {
      Regex::new(r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})|([0-9a-fA-F:]+:[0-9a-fA-F:]+)").unwrap()
  });
  ```
- [ ] `cargo test`.

#### 5.3 Покращення обробки `Mutex::lock()`
- [ ] Замінити `self.state.lock().unwrap()` на `self.state.lock().expect("State mutex poisoned")` у 14 місцях `app.rs`.
- [ ] Розглянути можливість використання `let Ok(state) = self.state.lock() else { return; };` для UI-функцій, щоб уникнути паніки при отруєному mutex.
- [ ] `cargo test`.

#### 5.4 Doc-коментарі на pub-елементах
- [ ] Додати `///` doc-коментарі до:
  - [ ] `HostStatus` — загальний опис ролі у системі.
  - [ ] `LogEntry` — опис кожного варіанту enum.
  - [ ] `EguiPinger` — загальний опис головного вікна.
  - [ ] `TracerouteHop` — опис полів.
  - [ ] `PingVisuals` — опис ролі.
  - [ ] Усіх `pub fn` у нових модулях `src/ui/`.
- [ ] `cargo doc --no-deps` — переконатися, що документація генерується без помилок.

#### 5.5 Фінальна валідація
- [ ] `cargo test` — усі тести проходять.
- [ ] `cargo clippy` — 0 попереджень.
- [ ] `cargo doc --no-deps` — без помилок.
- [ ] Запустити програму та перевірити всі UI-вікна вручну.

---

## Критерії успіху

| Критерій | Цільове значення |
|----------|-----------------|
| `ui_layout()` розмір | ≤ 150 рядків (зараз: 991) |
| `pinger_task()` розмір | ≤ 200 рядків (зараз: 589) |
| Магічних чисел | 0 (зараз: ~15 місць) |
| Дублювань коду | 0 основних (зараз: 5 патернів) |
| Clippy попереджень | 0 (зараз: 14) |
| Тестів failure deduction | ≥ 6 (зараз: 0) |
| Тестів incident detection | ≥ 3 (зараз: 0) |
| Doc-коментарі на pub-структурах | 100% (зараз: ~30%) |
| `cargo test` | ✅ усі проходять |
| Одна мова коментарів | English (зараз: mixed) |

---

## Залежності між фазами

```
Фаза 1 (Швидкі перемоги)
   │
   ├──► Фаза 2 (Рефакторинг UI)
   │
   └──► Фаза 3 (Рефакторинг пінгера)
            │
            └──► Фаза 4 (Тести логіки)
   
Фаза 5 (Техборг) — незалежна, можна робити паралельно з 2–4
```

Фази 2 та 3 можна виконувати **паралельно** — вони змінюють різні файли (`app.rs` vs `pinger.rs`). Фаза 4 залежить від Фази 3, бо тестує функції, створені під час декомпозиції пінгера.
