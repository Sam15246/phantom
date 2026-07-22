use futures_util::StreamExt;
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

// ---------------------------------------------------------------------------
// Transcription (OpenAI)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: String,
}

pub async fn transcribe_audio(api_key: &str, wav_bytes: Vec<u8>) -> Result<String, String> {
    let client = reqwest::Client::new();

    let part = multipart::Part::bytes(wav_bytes)
        .file_name("recording.wav")
        .mime_str("audio/wav")
        .map_err(|e| format!("MIME error: {e}"))?;

    let form = multipart::Form::new()
        .text("model", "gpt-4o-transcribe")
        .text("response_format", "json")
        .part("file", part);

    let response = client
        .post("https://api.openai.com/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {api_key}"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Transcription request error: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Transcription API error {status}: {body}"));
    }

    let result: TranscriptionResponse = response
        .json()
        .await
        .map_err(|e| format!("Transcription parse error: {e}"))?;

    Ok(result.text)
}

// ---------------------------------------------------------------------------
// Question Extraction & Mode Detection (Groq)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub question: String,
    pub mode: String,
    pub context: String,
}

pub async fn extract_question(groq_api_key: &str, transcript: &str) -> Result<ExtractionResult, String> {
    let client = reqwest::Client::new();

    let system_prompt = r#"You are an interview question extractor. Given a transcript, extract:
1. The core interview question (cleaned up)
2. The mode: one of "dsa", "system-design", "behavioral", "oop", "dbms", "general"
3. Any relevant context

Respond ONLY with JSON: {"question": "...", "mode": "...", "context": "..."}

Modes: dsa=algorithms/coding, system-design=architecture/HLD/LLD, behavioral=experience/STAR, oop=design patterns/SOLID, dbms=SQL/databases, general=everything else"#;

    let request = ChatRequest {
        model: "llama-3.3-70b-versatile".to_string(),
        messages: vec![
            ChatMessage { role: "system".to_string(), content: system_prompt.to_string() },
            ChatMessage { role: "user".to_string(), content: format!("Transcript:\n{transcript}") },
        ],
        temperature: 0.1,
        max_tokens: 500,
    };

    let response = client
        .post("https://api.groq.com/openai/v1/chat/completions")
        .header("Authorization", format!("Bearer {groq_api_key}"))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Groq request error: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Groq API error {status}: {body}"));
    }

    let result: ChatResponse = response.json().await.map_err(|e| format!("Groq parse error: {e}"))?;
    let content = result.choices.first().map(|c| c.message.content.clone()).unwrap_or_default();

    // Try to extract JSON from the response (it may have extra text around it)
    let json_str = if let Some(start) = content.find('{') {
        if let Some(end) = content.rfind('}') {
            &content[start..=end]
        } else {
            &content
        }
    } else {
        &content
    };

    serde_json::from_str::<ExtractionResult>(json_str)
        .map_err(|e| format!("Failed to parse extraction: {e}. Raw: {content}"))
}

pub fn fallback_extraction(transcript: &str) -> ExtractionResult {
    ExtractionResult {
        question: transcript.to_string(),
        mode: "general".to_string(),
        context: String::new(),
    }
}

// ---------------------------------------------------------------------------
// Answer Generation (streaming)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct StreamChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

fn select_model(mode: &str) -> &'static str {
    match mode {
        "dsa" => "o3",
        "system-design" => "gpt-4.1",
        "behavioral" => "gpt-4.1-mini",
        "oop" | "dbms" => "gpt-4.1",
        _ => "gpt-4.1",
    }
}

fn build_system_prompt(mode: &str, resume: &str, job_description: &str) -> String {
    let base = match mode {
        "dsa" => "You are an expert algorithm and data structures tutor. Provide optimal solutions with: approach, time/space complexity, clean code (Python unless asked otherwise), edge cases. Think step by step.",
        "system-design" => "You are a senior system design architect. Provide: requirements, high-level design, component deep-dive, data model, API design, scalability, trade-offs.",
        "behavioral" => "You are a career coach. Use STAR method (Situation, Task, Action, Result). Be specific, quantify impact. Keep answers 2-3 minutes spoken.",
        "oop" => "You are an OOP expert. Explain design patterns, SOLID principles with clear code examples and class relationships.",
        "dbms" => "You are a database expert. Provide SQL queries, normalization, indexing strategies, schema design.",
        _ => "You are a helpful technical interview coach. Provide clear, structured answers with code examples where appropriate.",
    };

    let mut prompt = base.to_string();
    if !resume.is_empty() {
        prompt.push_str(&format!("\n\nCandidate's background:\n{resume}"));
    }
    if !job_description.is_empty() {
        prompt.push_str(&format!("\n\nTarget role:\n{job_description}"));
    }
    prompt
}

pub async fn generate_answer_streaming(
    app: &AppHandle,
    api_key: &str,
    question: &str,
    mode: &str,
    context: &str,
    history: &[ChatMessage],
    resume: &str,
    job_description: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let model = select_model(mode);
    let system_prompt = build_system_prompt(mode, resume, job_description);

    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: system_prompt,
    }];

    for msg in history {
        messages.push(msg.clone());
    }

    let user_content = if context.is_empty() {
        question.to_string()
    } else {
        format!("Context from conversation:\n{context}\n\nQuestion:\n{question}")
    };

    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_content,
    });

    let request = StreamChatRequest {
        model: model.to_string(),
        messages,
        temperature: 0.3,
        max_tokens: 4096,
        stream: true,
    };

    let _ = app.emit("answer:mode", mode);

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Answer request error: {e}"))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Answer API error {status}: {body}"));
    }

    let mut full_answer = String::new();
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Stream error: {e}"))?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.starts_with("data: ") {
                let data = &line[6..];
                if data == "[DONE]" {
                    let _ = app.emit("answer:done", &full_answer);
                    return Ok(full_answer);
                }

                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                    if let Some(choice) = chunk.choices.first() {
                        if let Some(content) = &choice.delta.content {
                            full_answer.push_str(content);
                            let _ = app.emit("answer:chunk", content);
                        }
                    }
                }
            }
        }
    }

    let _ = app.emit("answer:done", &full_answer);
    Ok(full_answer)
}
