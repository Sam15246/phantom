"""
Phantom Health Check — run this to verify everything is working.
Tests: API keys, subscriptions, Groq extraction model, mode classification.

Usage: python test_phantom.py
"""
import os, json, hashlib, sys, time

try:
    import requests
    from cryptography.hazmat.primitives.ciphers.aead import AESGCM
except ImportError:
    print("Missing dependencies. Run: pip install requests cryptography")
    sys.exit(1)


def load_config():
    """Decrypt and load Phantom config."""
    p = os.path.join(os.environ.get("APPDATA", ""), "AudioDeviceManager", "config.enc")
    if not os.path.exists(p):
        print("ERROR: Config file not found. Have you set up Phantom settings?")
        return None
    d = open(p, "rb").read()
    machine = os.environ.get("COMPUTERNAME", "")
    user = os.environ.get("USERNAME", "")
    k = hashlib.sha256(f"phantom-{machine}-{user}-salt-v1".encode()).digest()
    try:
        return json.loads(AESGCM(k).decrypt(d[:12], d[12:], None))
    except Exception as e:
        print(f"ERROR: Decryption failed: {e}")
        return None


def get_groq_content(response_json):
    """Extract content from Groq response — reasoning models put output in 'reasoning' not 'content'."""
    msg = response_json["choices"][0]["message"]
    content = msg.get("content", "")
    if not content:
        content = msg.get("reasoning", "")
    return content


def test_openai(key):
    """Test OpenAI API key — used for transcription and answer generation."""
    try:
        r = requests.post("https://api.openai.com/v1/chat/completions",
            headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
            json={"model": "gpt-4o-mini", "messages": [{"role": "user", "content": "hi"}], "max_tokens": 5},
            timeout=15)
        if r.ok:
            return True, "WORKING"
        body = r.json().get("error", {}).get("message", r.text[:200])
        if "insufficient_quota" in str(body).lower():
            return False, f"NO CREDITS — needs recharge. Go to platform.openai.com/settings/organization/billing"
        elif "invalid" in str(body).lower() or "incorrect" in str(body).lower():
            return False, f"INVALID KEY — check your API key"
        else:
            return False, f"HTTP {r.status_code}: {body}"
    except Exception as e:
        return False, f"CONNECTION ERROR: {e}"


def test_groq(key):
    """Test Groq API key and extraction model — used for mode detection."""
    # First test key validity
    try:
        r = requests.get("https://api.groq.com/openai/v1/models",
            headers={"Authorization": f"Bearer {key}"}, timeout=10)
        if not r.ok:
            return False, f"INVALID KEY — HTTP {r.status_code}"
        models = [m["id"] for m in r.json().get("data", [])]
    except Exception as e:
        return False, f"CONNECTION ERROR: {e}"

    # Check if our extraction models exist
    primary = "openai/gpt-oss-120b"
    fallback = "openai/gpt-oss-20b"
    has_primary = primary in models
    has_fallback = fallback in models

    if not has_primary and not has_fallback:
        return False, f"NO EXTRACTION MODELS — neither {primary} nor {fallback} available. Available: {', '.join(sorted(models))}"

    # Test actual extraction
    test_model = primary if has_primary else fallback
    try:
        r = requests.post("https://api.groq.com/openai/v1/chat/completions",
            headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
            json={
                "model": test_model,
                "messages": [
                    {"role": "system", "content": 'Extract question and mode. Respond with JSON: {"question":"...","mode":"...","context":"..."}\nModes: ai-interview, dsa, backend, behavioral, system-design, qa, general'},
                    {"role": "user", "content": "Transcript:\nTell me about yourself and your experience"}
                ],
                "temperature": 0.1, "max_tokens": 1024
            }, timeout=20)
        if r.ok:
            content = get_groq_content(r.json())
            import re
            j = re.search(r'\{.*\}', content, re.DOTALL)
            if j:
                result = json.loads(j.group())
                mode = result.get("mode", "???")
                model_info = f"primary={primary}({'OK' if has_primary else 'MISSING'}), fallback={fallback}({'OK' if has_fallback else 'MISSING'})"
                return True, f"WORKING — tested with {test_model}, detected mode='{mode}'. Models: {model_info}"
            return False, f"PARSE ERROR — model responded but couldn't parse JSON from: {content[:100]}"
        elif r.status_code == 429:
            return True, f"KEY VALID but rate limited. Models available: {model_info}. This is normal for free tier — just wait a minute."
        else:
            return False, f"HTTP {r.status_code}: {r.text[:200]}"
    except Exception as e:
        return False, f"EXTRACTION TEST ERROR: {e}"


def test_classification(key):
    """Test mode classification accuracy with a few key cases."""
    cases = [
        ("Tell me about yourself", "ai-interview"),
        ("Tell me about a time you handled a conflict", "behavioral"),
        ("Design a URL shortener", "system-design"),
        ("What is the time complexity of binary search?", "dsa"),
        ("How do you handle caching in microservices?", "backend"),
        ("How do you write API test automation?", "qa"),
    ]

    import re
    passed = 0
    results = []

    for transcript, expected in cases:
        try:
            r = requests.post("https://api.groq.com/openai/v1/chat/completions",
                headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
                json={
                    "model": "openai/gpt-oss-120b",
                    "messages": [
                        {"role": "system", "content": 'Extract question and mode. Respond ONLY with JSON: {"question":"...","mode":"...","context":"..."}\nModes: ai-interview, dsa, oa, system-design, backend, java, python, dbms, cloud, ai-ml, behavioral, lld, qa, project-deep-dive, general, skip'},
                        {"role": "user", "content": f"Transcript:\n{transcript}"}
                    ],
                    "temperature": 0.1, "max_tokens": 1024
                }, timeout=20)

            if r.status_code == 429:
                results.append((transcript[:40], expected, "RATE-LIM", "-"))
                time.sleep(2)
                continue

            if not r.ok:
                results.append((transcript[:40], expected, "HTTP-ERR", "X"))
                continue

            content = get_groq_content(r.json())
            j = re.search(r'\{.*\}', content, re.DOTALL)
            if j:
                got = json.loads(j.group()).get("mode", "???")
                ok = got == expected
                if ok:
                    passed += 1
                results.append((transcript[:40], expected, got, "OK" if ok else "FAIL"))
            else:
                results.append((transcript[:40], expected, "PARSE-ERR", "X"))
        except Exception as e:
            results.append((transcript[:40], expected, str(e)[:20], "X"))

        time.sleep(1.5)

    return passed, len(cases), results


if __name__ == "__main__":
    print("=" * 60)
    print("  PHANTOM HEALTH CHECK")
    print("=" * 60)

    cfg = load_config()
    if not cfg:
        sys.exit(1)

    print()

    # --- OpenAI ---
    openai_key = cfg.get("openai_api_key", "")
    if not openai_key:
        print("[OpenAI]  NOT SET — this is required for transcription + answers!")
    else:
        masked = f"{openai_key[:7]}...{openai_key[-4:]}"
        print(f"[OpenAI]  Key: {masked} ({len(openai_key)} chars)")
        ok, msg = test_openai(openai_key)
        print(f"[OpenAI]  {msg}")

    print()

    # --- Groq ---
    groq_key = cfg.get("groq_api_key", "")
    if not groq_key:
        print("[Groq]    NOT SET — mode detection will use keyword fallback (less accurate)")
    else:
        masked = f"{groq_key[:7]}...{groq_key[-4:]}"
        print(f"[Groq]    Key: {masked} ({len(groq_key)} chars)")
        ok, msg = test_groq(groq_key)
        print(f"[Groq]    {msg}")

        # Classification test
        if ok and "--quick" not in sys.argv:
            print(f"\n--- Mode Classification Test ---")
            passed, total, results = test_classification(groq_key)
            print(f"{'Transcript':<42} {'Expected':<16} {'Got':<16} {'Status'}")
            print("-" * 90)
            for t, exp, got, status in results:
                print(f"{t:<42} {exp:<16} {got:<16} {status}")
            print("-" * 90)
            actual_tested = sum(1 for _, _, _, s in results if s in ("OK", "FAIL"))
            print(f"Result: {passed}/{actual_tested} correct" + (" (some skipped due to rate limit)" if actual_tested < total else ""))

    print()
    print("=" * 60)
    print("  Done. Run with --quick to skip classification tests.")
    print("=" * 60)
