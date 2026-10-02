# AGENTS.md — Руководство для AI-агентов (Spec-Driven Design)

Репозиторий разрабатывается по методологии **Spec-Driven Design (SDD)**. 
Правила игры, серверная архитектура и API зафиксированы в [`docs/`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs). Для автономного клиента-визуализатора `visualizer_2` его локальные архитектурные, доменные и технические спецификации находятся в [`visualizer_2/docs/`](file:///Users/d.byta/Documents/Code/dats/datsmagic/visualizer_2/docs); серверные контракты остаются авторитетными в корневой документации.

---

## 1. Роль директории `docs/` (Single Source of Truth)

- Каталог [`docs/`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs) является **источником правды по правилам игры, серверу и API**.
- Каталог [`visualizer_2/docs/`](file:///Users/d.byta/Documents/Code/dats/datsmagic/visualizer_2/docs) является источником правды **только по реализации и интерфейсу Rust-визуализатора**; он не может переопределять игровые правила и API.
- Любые алгоритмы, структуры данных, клиенты API, серверные модули и стратегии ботов должны строго следовать соответствующим спецификациям этих каталогов.
- Запрещено зашивать в код скрытые эвристики, допущения или «магические числа» в обход спецификаций.

---

## 2. Структура документации проекта

| Раздел / Директория | Назначение |
| :--- | :--- |
| [`docs/mechanics.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/mechanics.md) | **Базовая механика**: правила симулятора DatsMagic, физическая модель (Эйлер, трение, векторы) и спецификация REST API. |
| [`docs/adr/`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/adr) | **Архитектурные решения (ADR)**: выбор технологий, языков программирования, принципов структурирования кода и взаимодействия компонентов. |
| [`docs/domain/`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain) | **Записи предметных областей (Domain Records)**: декомпозиция функциональности по принципу единой ответственности (SRP) с точки зрения бизнес-логики и правил. |
| [`docs/features/`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features) | **Технические спецификации фичей (Feature Specs)**: детальный дизайн модулей, структуры данных, алгоритмы, DTO и планы реализации. |
| [`visualizer_2/docs/`](file:///Users/d.byta/Documents/Code/dats/datsmagic/visualizer_2/docs) | **Изолированные спецификации Rust-визуализатора**: локальная ADR, доменная запись и FE-001...; нумерация начинается с 1. |

---

## 3. Обязательные шаблоны для будущих сессий

При создании новых сущностей, модулей или фичей агент обязан использовать стандартные шаблоны репозитория:

- **Шаблон Domain Record**: [`docs/DOMAIN_RECORD_TEMPLATE.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/DOMAIN_RECORD_TEMPLATE.md) — используется для формализации новых функциональных участков и предметных областей в `docs/domain/`. Каждый документ обязан следовать **Single Responsibility Principle (SRP)**.
- **Шаблон Feature Specification**: [`docs/FEATURE_TEMPLATE.md`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/FEATURE_TEMPLATE.md) — используется для проектирования конкретных технических решений и алгоритмов в `docs/features/`.

---

## 4. Рабочий процесс агента (Docs-First Workflow)

1. **Изучение спецификаций перед кодингом**:
   - Перед реализацией или модификацией кода агент обязан изучить связанные документы в [`docs/adr/`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/adr), [`docs/domain/`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/domain) и [`docs/features/`](file:///Users/d.byta/Documents/Code/dats/datsmagic/docs/features).
2. **Спецификация первична (Docs-First)**:
   - При добавлении функциональности сначала создается/обновляется Domain Record или Feature по соответствующему шаблону, и только затем изменения вносятся в код.
3. **Тестирование по спецификации**:
   - Тесты и валидация строятся на основе инвариантов, формул, контрактов и Acceptance Criteria из `docs/`.
4. **Синхронизация**:
   - Любое расхождение между кодом и `docs/` трактуется как дефект реализации, если только явно не стоит задача изменить саму спецификацию.
