# Дослідження: Якість коду egui_pinger

## Тема дослідження
Аудит якості коду проєкту egui_pinger: виявлення типових проблем, що накопичилися при розробці, та рекомендації по рефакторингу до рівня «підтримуваного» коду.

## Мета
Системно оцінити код за п'ятьма критеріями: **модульність**, **дублювання коду**, **магічні числа й константи**, **якість коментарів**, **тестове покриття** — і сформувати конкретні кроки для покращення.

---

## 1. Модульність: «God Function» у `app.rs`

### Проблема
Файл `src/app.rs` — **1235 рядків**. Але найбільша проблема — це метод `ui_layout()`: **991 рядків** (227–1218). Він відповідає за ВСЕ:

- Панель додавання хоста (рядки 229–305)
- Основний список хостів із графіками (рядки 317–605)
- Обробку Drag & Drop (рядки 608–614)
- Обробку зупинки моніторингу (рядки 616–654)
- Діалог підтвердження видалення (рядки 657–693)
- Вікно налаштувань хоста (рядки 695–850)
- Модальне вікно трасування маршруту (рядки 852–910)
- Довідку по метрикам (рядки 912–1005)
- Вікно журналу подій (рядки 1012–1214)

Це класична «God Function» — одна функція, яка робить все. Будь-яку зміну в одній частині UI складно робити, бо треба розуміти весь контекст.

### Аналогічна проблема: `pinger_task()` в `pinger.rs`
Функція `pinger_task()` — **589 рядків** (45–634). Один `loop {}` з 4 великими секціями:
1. Перевірка traceroute (рядки 60–160)
2. Діагностика DOWN-хостів (рядки 162–196)
3. Планування пінгів (рядки 198–268)
4. Failure Deduction — складна логіка з 5 рівнями вкладеності (рядки 270–388)
5. Виконання пінгів та обробка результатів (рядки 390–629)

### Рекомендація
**Варіант A (мінімальний):** Виокремити кожне вікно/діалог у окрему функцію в тому ж файлі.
```rust
// app.rs
fn render_host_settings_window(&mut self, ctx: &egui::Context, visuals: &PingVisuals) { ... }
fn render_route_window(&mut self, ctx: &egui::Context, visuals: &PingVisuals) { ... }
fn render_log_window(&mut self, ctx: &egui::Context, visuals: &PingVisuals) { ... }
fn render_help_window(&mut self, ctx: &egui::Context) { ... }
fn render_host_row(&self, ui: &mut egui::Ui, host: &HostInfo, status: &HostStatus, ...) { ... }
```

**Варіант B (повний):** Перенести UI кожного вікна в окремий файл `src/ui/`:
```
src/ui/
├── mod.rs
├── system_tools.rs    # вже є
├── host_settings.rs   # вікно налаштувань хоста
├── route_viewer.rs    # вікно трасування
├── log_viewer.rs      # вікно журналу
├── help.rs            # довідка по метрикам
└── host_row.rs        # рендеринг одного рядка хоста з графіком
```

Аналогічно для `pinger_task()`:
```rust
fn check_traceroute_targets(state: &SharedState, ...) -> Vec<String> { ... }
fn update_diagnostic_mode(state: &SharedState, ...) { ... }
fn collect_addresses_to_ping(state: &SharedState, ...) -> Vec<(String, PingMode, ...)> { ... }
fn deduce_failure_points(state: &SharedState, ...) { ... }
fn process_ping_result(state: &SharedState, address: &str, alive: bool, rtt_ms: f64, ...) { ... }
```

**Рекомендовано: Варіант B** — він дає кращу ізоляцію і значно покращує навігацію по коду.

---

## 2. Дублювання коду (DRY — Don't Repeat Yourself)

### 2.1 Кольори `LogEntry` — повна копія
Блок визначення кольору для записів журналу **повністю дублюється** у рядках 1114–1134 та 1179–1198:
```rust
let color = match entry {
    LogEntry::Ping { rtt: None, .. } => Color32::from_rgb(230, 159, 0),
    LogEntry::Incident { is_break: true, .. } => Color32::from_rgb(213, 94, 0),
    LogEntry::Incident { is_break: false, .. } => Color32::from_rgb(0, 158, 115),
    LogEntry::Statistics { .. } => Color32::from_rgb(0, 158, 115),
    LogEntry::RouteUpdate { .. } => Color32::from_rgb(0, 114, 178),
    LogEntry::Marker { .. } => Color32::from_rgb(204, 121, 167),
    _ => visuals.latency_color(0.1),
};
```
Це 100% однаковий код. Потрібна одна функція `log_entry_color()`.

### 2.2 Обрізка `events` до 100_000
Один і той самий паттерн зустрічається **5 разів** у різних файлах:
```rust
while status.events.len() > 100_000 {
    status.events.pop_front();
}
```
Місця: `app.rs:205`, `app.rs:650`, `pinger.rs:136`, `pinger.rs:538`, `pinger.rs:601`.

Це має бути метод `HostStatus::trim_events()`.

### 2.3 Файлове логування
Патерн «відкрити файл → записати рядок» повторюється **4 рази** в `pinger.rs` та `app.rs`:
```rust
if let Ok(mut file) = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open(&h.log_file_path) { ... }
```
Варто зробити метод `HostInfo::append_to_log(&self, line: &str)`.

### 2.4 Перетворення `PingMode` в текст
`PingMode` -> текстовий опис дублюється двічі для ComboBox у вікні налаштувань (рядки 724–731 та 734–740):
```rust
// Перший раз — для відображення обраного значення
.selected_text(match h.mode {
    PingMode::VeryFast => tr!("Very fast (1s)"),
    ...
})
// Другий раз — для списку варіантів
.show_ui(ui, |ui| {
    ui.selectable_value(&mut h.mode, PingMode::VeryFast, tr!("Very fast (1s)"));
    ...
});
```
Варто імплементувати `impl Display for PingMode` або метод `PingMode::label()`.

### 2.5 Обробка пінгу «не вдалося створити Requestor» (pinger.rs)
Два блоки в `pinger.rs` (рядки 460–561 та 564–627) дублюють логіку: `add_sample`, створення `LogEntry::Ping`, детекція інцидентів, обрізку подій, файлове логування. Другий блок — це «невдалий» шлях (requestor == None). Вся ця логіка має бути винесена у спільну функцію.

### Рекомендація
Впровадити **5 нових функцій/методів**, які усунуть 80% дублювання:
1. `fn log_entry_color(entry: &LogEntry, visuals: &PingVisuals) -> Color32`
2. `HostStatus::trim_events()`
3. `HostInfo::append_to_log(&self, lines: &[String])`
4. `PingMode::label(&self) -> String`
5. `fn process_ping_result(state: &mut AppState, address: &str, alive: bool, rtt_ms: f64, host_info: Option<&HostInfo>) -> Vec<LogEntry>`

---

## 3. Магічні числа та відсутність констант

### Проблема
По всьому коду розкидані числа без пояснень:

| Число | Де зустрічається | Що означає |
|-------|-----------------|------------|
| `300` | `status.rs:521`, `pinger.rs:512` | Розмір ковзного вікна |
| `100_000` | 5 місць | Максимум подій в журналі |
| `10_000` | `app.rs:1093` | Максимум подій для UI |
| `150.0` | `app.rs:512`, `app.rs:539` | Поріг «попередження» (мс) |
| `3` | `pinger.rs:76,293,317,336,482,585` | Streak для «підтвердження стану» |
| `16.0` | `status.rs:576` | Дільник RFC 3550 |
| `15` | `pinger.rs:314,333,348,350` | Тайм-аут свіжості (сек) |
| `3600` | `pinger.rs:81` | Інтервал traceroute (сек) |
| `60` | `pinger.rs:83` | Мінімальний інтервал re-trace |

### Рекомендація
Створити модуль `src/constants.rs` (або блок `const` в `lib.rs`):
```rust
/// Size of the sliding window for RTT history.
pub const HISTORY_WINDOW_SIZE: usize = 300;

/// Maximum number of events stored per host.
pub const MAX_EVENTS_PER_HOST: usize = 100_000;

/// Maximum events shown in the UI log viewer.
pub const MAX_UI_EVENTS: usize = 10_000;

/// RTT threshold (ms) for the "warning" line on the chart.
pub const RTT_WARNING_THRESHOLD: f64 = 150.0;

/// Number of consecutive failures to confirm "DOWN" status.
pub const DOWN_CONFIRMATION_STREAK: u32 = 3;

/// Freshness timeout for hop data (seconds).
pub const HOP_DATA_FRESHNESS_SEC: u64 = 15;

/// Interval between periodic traceroutes (seconds).
pub const TRACEROUTE_INTERVAL_SEC: u64 = 3600;

/// Minimum re-trace cooldown after status change (seconds).
pub const TRACEROUTE_MIN_COOLDOWN_SEC: u64 = 60;

/// Statistics snapshot interval (every N pings).
pub const STATS_SNAPSHOT_INTERVAL: u32 = 300;
```

---

## 4. Якість коментарів

### 4.1 Змішання мов
Коментарі хаотично перемикаються між українською та англійською:
```rust
// Ручка для перетягування       (рядок 473 — українська)
// add host to the list          (рядок 252 — англійська)
// Перемикач тем та кнопка...    (рядок 294 — українська)
// Cap buffer size               (рядок 537 — англійська)
```

### 4.2 Коментарі «що», а не «навіщо»
```rust
// Add to history (maximum 300 samples)   // очевидно зі self.history.push(rtt_ms)
self.history.push(rtt_ms);
if self.history.len() > 300 {
    self.history.remove(0);
}
```
Некорисний коментар — описує те, що і так видно з коду. Корисний коментар пояснив би, **чому** саме 300 і чому `remove(0)` а не `VecDeque`.

### 4.3 Відсутні doc-коментарі на pub-елементах
- `HostStatus` (45 полів) — більшість полів має `///` коментарі, **але сама структура** не має документації, що пояснює її роль у системі.
- `LogEntry` — немає документації на варіантах enum (тільки одне `/// Custom message marker` для `Marker`).
- `EguiPinger` — немає загальної документації, окрім полів.
- `TracerouteHop` — є поля, але нуль doc-коментарів.

### 4.4 Помилка в коментарі
`status.rs:382,384,392`: коментарі до полів `history` та `rtp_jitter_history` кажуть «last 99», але фактичний ліміт — 300.

### Рекомендація
1. **Обрати одну мову для коментарів** — згідно зі специфікацією: інтерфейс англійський → коментарі англійською.
2. **Замінити «що»-коментарі на «навіщо»-коментарі**.
3. **Додати doc-коментарі** (///) до всіх pub-структур, enum та методів.
4. **Виправити неточності** (99 → 300).

---

## 5. `HostStatus` — «God Struct»

### Проблема
`HostStatus` має **45 полів**. Він зберігає ВСЕ: RTT-статистику, jitter, MOS, streak, traceroute-шлях, діагностику, журнал подій, інциденти. Це порушує принцип Single Responsibility.

### Рекомендація
Розбити на логічні групи:

```rust
pub struct HostStatus {
    pub latency: LatencyStats,        // mean, median, p95, stddev, min, max, history
    pub jitter: JitterStats,          // rtp_jitter, mean, median, history
    pub quality: QualityMetrics,      // mos, availability, outliers
    pub connectivity: ConnectivityState, // alive, streak, streak_success, sent, lost
    pub traceroute: TracerouteState,  // path, is_trace_hop, dependent_targets, etc.
    pub incidents: IncidentTracker,   // failure_point, incident_start, prev_alive
    pub log: EventLog,               // events, log_pings_since_stats
}
```

Це зробить код значно читабельнішим і дозволить тестувати кожну групу окремо.

---

## 6. Проблеми з продуктивністю

### 6.1 `Vec::remove(0)` — O(n), треба `VecDeque`
```rust
// status.rs:522
self.history.push(rtt_ms);
if self.history.len() > 300 {
    self.history.remove(0);  // O(300) на кожне видалення!
}
```
Те саме для `rtp_jitter_history` (рядок 581). При 300 елементах це не критично, але це поганий паттерн. `VecDeque` з `push_back` + `pop_front` — O(1). Крім того, `calculate_percentile` створює копію для сортування щоразу — краще використовувати не-сортуючий алгоритм.

### 6.2 Regex::new в кожному виклику
```rust
// tracer.rs:70
let ip_re = Regex::new(r"...").unwrap();  // Компілюється КОЖЕН раз!
```
Треба `lazy_static!` або `std::sync::LazyLock`:
```rust
static IP_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"...").unwrap());
```

---

## 7. Залишки clippy

`cargo clippy` видає **14 попереджень**:
- 7× `collapsible_if` — вкладені if, які можна об'єднати
- 1× `unwrap_or_default` — `or_insert_with(Default::default)` → `or_default()`
- 6× інші дрібні стилістичні проблеми

Все вирішується  `cargo clippy --fix --lib`.

---

## 8. Тестове покриття

### Що добре
- `model/status_tests.rs` — **44 тести** для статистичного ядра (mean, median, MOS, jitter, outliers, percentile, streak). Це найкраще покрите місце.
- `tests/gui_tests.rs` — **22 GUI-тести** (відкривання вікон, додавання хостів, валідація, D&D).
- `ui/system_tools.rs` — 3 unit-тести для списку команд.

### Що погано
- **`pinger.rs` — 0 тестів логіки**. Вся логіка traceroute-оновлений, failure deduction, планування пінгів не протестована. Тести тільки перевіряють payload generation і parsing, але не бізнес-логіку.
- **Failure deduction (рядки 270–388)** — найскладніший алгоритм у проєкті, **0 тестів**. Будь-яка помилка тут непомітна.
- **Incident detection (рядки 482–507, 582–596)** — 0 тестів.
- **MOS integration** — тест `test_calculate_mos_values` тестує формулу, але не тестує, як `add_sample` викликає `calculate_mos` з правильними параметрами.

### Рекомендація
Створити `tests/pinger_logic_tests.rs` з юніт-тестами для:
1. Failure deduction: передати стан із відомими hop-ами та перевірити, що `failure_point` визначається правильно.
2. Incident detection: перевірити, що 3 невдачі створюють `Incident { is_break: true }`, а потім успіх — `Incident { is_break: false }`.
3. Traceroute update logic: перевірити, що «порожній traceroute не перезаписує валідний».

Для цього потрібно спочатку **витягнути логіку з `pinger_task()`** у тестовані функції (п. 1).

---

## 9. Безпека та обробка помилок

### 9.1 `self.state.lock().unwrap()` — 14 місць
У `app.rs` є **14 викликів `lock().unwrap()`**. Якщо mutex отруєний (наприклад, pinger thread запанікував), весь UI крашнеться. Це маловірогідно, але краще обробляти:
```rust
// Замість:
let state = self.state.lock().unwrap();
// Використовувати:
let state = self.state.lock().expect("State mutex poisoned");
// Або навіть:
let Ok(state) = self.state.lock() else { return; };
```

### 9.2 `Regex::new(...).unwrap()` в runtime
`tracer.rs:70` — regex компілюється при кожному виклику. Якщо паттерн невалідний, паніка.

---

## Підсумкова таблиця проблем

| # | Проблема | Серйозність | Складність виправлення |
|---|---------|-------------|----------------------|
| 1 | `ui_layout()` — 991 рядків | 🔴 Висока | Середня |
| 2 | `pinger_task()` — 589 рядків | 🔴 Висока | Середня |
| 3 | Дублювання кольорів журналу | 🟡 Середня | Низька |
| 4 | Дублювання trim_events ×5 | 🟡 Середня | Низька |
| 5 | Дублювання файлового логування ×4 | 🟡 Середня | Низька |
| 6 | Магічні числа (300, 100K, 150, 3) | 🟡 Середня | Низька |
| 7 | Коментарі неточні (99→300) | 🟢 Низька | Низька |
| 8 | Змішані мови коментарів | 🟢 Низька | Низька |
| 9 | `HostStatus` — 45 полів | 🟡 Середня | Висока |
| 10 | Vec::remove(0) замість VecDeque | 🟢 Низька | Низька |
| 11 | Regex без кешування | 🟢 Низька | Низька |
| 12 | Failure deduction — 0 тестів | 🔴 Висока | Середня |
| 13 | 14× clippy warnings | 🟢 Низька | Низька (auto-fix) |
| 14 | Дублювання PingMode::label | 🟢 Низька | Низька |
| 15 | process_ping_result дублювання | 🟡 Середня | Середня |

---

## Рекомендований порядок виправлення

### Фаза 1: Швидкі перемоги (1–2 години)
1. **Clippy auto-fix** — `cargo clippy --fix --lib`
2. **Константи** — створити `src/constants.rs` та замінити магічні числа
3. **`trim_events()`** — метод у `HostStatus`, замінити 5 дублювань
4. **`log_entry_color()`** — одна функція замість двох блоків
5. **Виправити коментарі** (99→300, обрати мову)

### Фаза 2: Рефакторинг UI (2–4 години)
6. **Розбити `ui_layout()`** на методи: `render_host_settings`, `render_route_window`, `render_log_window`, `render_help_window`
7. Перемістити кожний метод у відповідний файл `src/ui/`
8. **`PingMode::label()`** — Display trait

### Фаза 3: Рефакторинг логіки пінгера (3–4 години)
9. **Розбити `pinger_task()`** на 5+ функцій
10. **`process_ping_result()`** — одна функція замість двох гілок
11. **`HostInfo::append_to_log()`** — заміна 4 дублювань
12. **`HostStatus` → підструктури** — LatencyStats, JitterStats, etc.

### Фаза 4: Тести для логіки (2–3 години)
13. **Failure deduction тести** — найкритичніше
14. **Incident detection тести**
15. **Traceroute update тести**

### Фаза 5: Техборг (1 година)
16. `Vec<f64>` → `VecDeque<f64>` для history
17. `LazyLock` для Regex
18. Doc-коментарі на pub-елементах

---

## Вплив на проєкт

### Файли, які будуть змінені:
- `src/app.rs` — основна мета рефакторингу (1235→~400 рядків після виокремлення)
- `src/logic/pinger.rs` — декомпозиція на функції (639→~200 рядків)
- `src/model/status.rs` — додавання методів, можливе розбиття структури
- `src/constants.rs` — **новий файл**
- `src/ui/host_settings.rs` — **новий файл**
- `src/ui/route_viewer.rs` — **новий файл**
- `src/ui/log_viewer.rs` — **новий файл**
- `src/ui/help.rs` — **новий файл**
- `src/ui/host_row.rs` — **новий файл**

### Що НЕ треба чіпати:
- `src/main.rs` — вже чистий і компактний (57 рядків)
- `src/model/app_state.rs` — простий і стабільний
- `src/logic/tracer.rs` — маленький і чистий (144 рядки)
- Тести — додавати нові, а не переписувати існуючі
