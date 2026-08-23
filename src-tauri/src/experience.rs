/// Structured professional experience — embedded at compile time.
/// Each section is pulled by relevant modes in build_system_prompt().

/// Backend development experience — Java, Spring Boot, microservices, RxJava, APIs
pub const BACKEND_DEV: &str = r#"
=== BACKEND DEVELOPMENT EXPERIENCE ===

**Current Role:** Software Engineer at HSBC Technology India (July 2024 – Present, Pune)

**Core Stack:** Java, Spring Boot, REST APIs, microservices, RxJava, reactive programming

**What I do day-to-day:**
- Build and enhance Java/Spring Boot backend services and REST APIs for banking, card-processing and digital-payment workflows
- Work on process orchestration APIs (PAPI) that coordinate multiple internal banking systems and external payment services within complex customer journeys
- Develop backend API flows involving multiple downstream services, request/response transformations, conditional processing and business-state management
- Work across API controllers, service-layer logic, request/response models and downstream integration components
- Implement integrations with internal banking platforms and external payment networks including Visa services
- Use RxJava for reactive and asynchronous downstream service coordination
- Implement sequential, conditional and asynchronous API execution across distributed services
- Handle retry, timeout and exception/error-handling for resilient downstream execution
- Work with OAuth, JWT and B2B token-based authentication for secure service-to-service communication
- Develop success, failure and recovery paths across distributed service integrations
- Handle backend state transitions for enrollment, opt-out, processing and failure workflows
- Write unit and component-level tests using JUnit and Mockito
- Use Jenkins CI/CD pipelines, Docker/Podman, Kubernetes for deployment
- Use Git/GitHub for version control, Splunk for log analysis and troubleshooting
- Built a Python utility to extract, parse and aggregate Splunk logs for simplifying failure investigation
- Worked on API modernization: Nova migration (200+ APIs across team, personally handled 3), Mule→Kong gateway migration, AL2→AL3 platform migration

**Key Technologies Used:**
- RxJava: flatMap, observeOn(Schedulers.io()) for parallel downstream calls, switchMap for cancellation, zip for combining responses, retry with backoff
- Spring Boot: @RestController, @Service, @Transactional, RestTemplate/WebClient for downstream calls, Spring Security with OAuth2 resource server
- Testing: JUnit 5 + Mockito — mock downstream services, verify interaction counts, ArgumentCaptor for request validation
- Containerization: Dockerfiles with multi-stage builds, Kubernetes deployments with health probes, resource limits
"#;

/// QA/SDET experience — API automation, test design, mobile testing
pub const QA_SDET: &str = r#"
=== QA / SDET EXPERIENCE ===

**Automation Stack:** Java, Cucumber, Gherkin, JUnit for BDD-style API automation

**What I do:**
- Design, develop and maintain API automation test scenarios using Java + Cucumber + Gherkin + JUnit
- Create BDD-style Gherkin feature files and Java step definitions covering functional, negative, boundary and edge-case scenarios
- Build reusable automation components for API invocation, request preparation, response validation and assertion
- Perform REST API testing using Postman and Insomnia — validating payloads, HTTP status codes, headers, auth, error responses
- Test microservice-based backend workflows involving multiple downstream services
- Design end-to-end test scenarios for multi-step API orchestration flows
- Validate request/response transformations between upstream APIs, orchestration layers and downstream services
- Test downstream failures, invalid inputs, unexpected responses, partial failures, retry behaviour, timeout conditions, async processing
- Perform functional, integration, regression, system, E2E, smoke, sanity and exploratory testing
- Use Splunk logs to trace API flows across distributed services and investigate downstream failures
- Use JIRA for defect management, Confluence for documentation
- Work within Agile/Scrum teams

**Mobile Testing:**
- Appium for mobile automation testing
- Android Studio for Android testing/debugging, Xcode for iOS testing
- Test mobile customer journeys while validating underlying backend APIs
- Correlate mobile app behaviour with backend API responses and application logs

**API Automation Architecture:**
- BDD feature files → Java step definitions → reusable API client components → assertions
- Assertions cover: HTTP status codes, response payloads, business rules, backend states, error responses, downstream service responses
- Automated positive, negative, boundary and edge-case scenarios
- Regression coverage maintained for all API changes

**Migration Testing:**
- Nova migration: functional + regression testing, API request/response compatibility validation, downstream integration verification
- Mule→Kong: API gateway behaviour validation, downstream integration testing after migration
- AL2→AL3: compatibility testing, troubleshooting platform-specific issues

**Testing Types Expertise:**
Functional | Integration | Regression | System | End-to-End | Smoke | Sanity | Exploratory | Negative | Edge-Case | Migration | Mobile | API Automation
"#;

/// Key project details — specific enough for deep-dive and behavioral answers
pub const PROJECTS: &str = r#"
=== KEY PROJECTS ===

**1. Visa Click-to-Pay / In-App Card Provisioning — HSBC UK**
Stack: Java, Spring Boot, REST APIs, RxJava, Microservices, OAuth, JWT, Kubernetes, Jenkins

What it is: Backend implementation of Visa Click-to-Pay card-provisioning capability for HSBC UK banking ecosystem.

What I built/worked on:
- Process-orchestration APIs supporting customer eligibility, enrollment, opt-out and status-processing journeys
- Multi-step service flows: customer-data retrieval → card-system integration → Visa data retrieval → enrollment → payment-instrument provisioning
- Orchestration across CDM (customer data management), RPS and Visa services
- Request/response transformations between upstream orchestration APIs and downstream systems
- Conditional service execution based on downstream responses
- Customer/card identifier operations (ECID) during enrollment workflows
- Enrollment flows where customer info was created/retrieved before initiating Visa processing
- Failure and cleanup paths for unsuccessful external enrollment operations
- RxJava-based reactive flows for downstream orchestration and async processing
- Asynchronous polling to retrieve external Visa request status and trigger backend processing
- Backend status transitions:
  * 02 — Enrollment in Progress
  * 03 — Successfully Enrolled / Notification Sent
  * 04 — Successfully Enrolled / Notification Not Sent
  * 09 — Enrollment Failed
  * 12 — Opt-Out in Progress
  * 13 — Opt-Out Successfully Completed
- OAuth/JWT and B2B token-based auth for service-to-service communication
- Error handling, retry and timeout mechanisms around downstream integrations
- Deployment through Jenkins CI/CD and Kubernetes environments
- Used Splunk for tracing failures across the distributed Click-to-Pay backend

QA work on this project:
- Tested eligibility scenarios across multiple backend states (eligible, non-eligible, enrollment/opt-out in-progress)
- Validated multi-service enrollment workflow end-to-end
- Tested CDM, RPS and Visa service orchestration — validated data passed between downstream APIs
- Validated ECID creation and reuse flows
- Tested failure scenarios where Visa enrollment failed and validated backend cleanup/rollback
- Tested async polling workflows for Visa request status
- Designed negative/edge-case scenarios: eligibility failures, downstream failures, invalid states, incomplete responses
- Validated API behaviour for different downstream response codes and payloads
- API automation with Java + Cucumber + Gherkin + JUnit

**2. Corporate Cards — HSBC MENA**
Stack: Java, Spring Boot, REST APIs, Microservices, API Integration, Kubernetes

What it is: Backend services for Corporate Cards platform for MENA region.

Card management journeys: Card Summary, Card Details, Block Card, Unblock Card, Lost/Stolen Card, Set Card PIN, PIN Reset, Zone PIN Key
Transaction journeys: Posted Transactions, Unposted Transactions, Authorised Transactions, Declined Transactions

What I did:
- API orchestration and downstream service integration for card management
- Request/response models and backend integration flows
- Backend changes, change requests, deployment, issue investigation
- Both development AND testing across all card management and transaction flows
- Mobile app testing correlated with backend API responses

**3. Nova API Modernization / Migration**
- Large-scale migration of 200+ APIs across team
- Personally worked on 3 APIs
- Analysed existing API behaviour and downstream dependencies
- Validated request/response compatibility during migration
- Post-migration verification across environments

**4. Platform Migrations**
- Mule → Kong: API gateway behaviour changes, downstream integration validation
- AL2 → AL3: Platform compatibility changes, troubleshooting environment issues

**5. Data Analyst Agent — Personal AI Project**
Stack: Python, FastAPI, LangChain, Gemini API, Agentic AI

- Built autonomous agentic system that generates, executes and validates Python code to answer analytical questions over uploaded datasets
- LLM agent using LangChain's tool-calling framework
- Multi-model fallback and API-key rotation across Gemini tiers
- Isolated code execution using subprocess-based sandboxing with execution timeouts
- Retry logic, quota-aware key rotation, graceful model degradation, error recovery

**6. Virtual TA — RAG Chatbot**
Stack: FastAPI, GPT-4o, LangChain, OpenAI API, SQLite, RAG

- End-to-end RAG chatbot for IIT Madras' Tools in Data Science course
- Data-processing pipeline to ingest structured/unstructured educational content
- 2,255 embedded chunks indexed into SQLite with cosine-similarity retrieval
- Multimodal question answering, metadata logging, LLM-based response generation
- LLM-as-judge evaluation workflow, deployed on Vercel
"#;

/// Domain expertise — banking, cards, payments
pub const DOMAIN: &str = r#"
=== DOMAIN EXPERTISE ===

**Banking:** Digital banking platforms, banking APIs, customer-data systems (CDM), card-management services, backend banking integrations

**Cards:** Corporate Cards, debit/credit card workflows, Global Money Card, card management (block/unblock, lost/stolen), card provisioning, card enrollment, PIN workflows (set/reset/zone PIN key), card transactions (posted/unposted/authorised/declined)

**Payments:** Payment-processing workflows, external payment-network integrations, Visa integrations, Visa Click-to-Pay, payment-instrument provisioning, customer enrollment/opt-in/opt-out workflows, asynchronous payment processing, backend status management

**Markets/Regions:** India, Singapore, UAE, MENA/MENAT, United Kingdom

**HSBC Entities:** UKRB, HBEU, CIIOM, GB-HRFB (separate non-HSBC entity)

**Education:**
- B.Tech Computer Engineering — Jamia Millia Islamia, New Delhi (2020-2024), CGPA 9.54/10, Top 5%, Merit Scholarship
- Diploma in Data Science & Programming — IIT Madras Online (2023-2025), CGPA 8.34/10

**Certifications:**
- Oracle Certified Java SE 8 Programmer (2024)
- Python Certified Associate Programmer PCAP (2025)
- Databricks Certified Data Engineer Associate (2025)
"#;

/// Returns experience sections relevant to a given mode
pub fn experience_for_mode(mode: &str) -> String {
    match mode {
        "backend" | "java" => format!("{}\n{}\n{}", BACKEND_DEV, PROJECTS, DOMAIN),
        "qa" => format!("{}\n{}\n{}", QA_SDET, PROJECTS, DOMAIN),
        "project-deep-dive" => format!("{}\n{}\n{}\n{}", BACKEND_DEV, QA_SDET, PROJECTS, DOMAIN),
        "ai-interview" => format!("{}\n{}\n{}\n{}", BACKEND_DEV, QA_SDET, PROJECTS, DOMAIN),
        "behavioral" => format!("{}\n{}", PROJECTS, DOMAIN),
        "system-design" | "lld" => format!("{}\n{}", BACKEND_DEV, PROJECTS),
        "ai-ml" | "cloud" => PROJECTS.to_string(),
        _ => String::new(),
    }
}
