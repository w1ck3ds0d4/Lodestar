//! Offline, deterministic scam-job scoring. Computed at ingest (in `upsert_jobs`)
//! from a posting's own fields: no network, no LLM. Two tiers:
//!
//! - HARM triggers (35-40 pts) are near-unique to fraud (upfront fee, pre-hire PII,
//!   money-mule duties, fake-check/overpayment, task-deposit/crypto, reshipping,
//!   brand-impersonation via free webmail). Any one pushes a posting to "review".
//! - WEAK signals (8-18 pts) are individually legal/common, capped, and FP-prone.
//!
//! Guards keep legitimate postings clear of the line: trusted employer ATS feeds are
//! exempt from all weak checks, aggregator feeds suppress the structural ones, the
//! three purely-structural weak checks are jointly capped unless a content signal
//! also fires, and "likely scam" requires at least one harm trigger. The design was
//! adversarially stress-tested against legit-but-scammy-looking postings (commission
//! sales, founding-engineer, staffing recruiters, etc.), which all stay below 40.

use std::sync::OnceLock;

use regex::Regex;

/// Advisory score (>= this) worth a "review" badge. The badge tiers are applied in
/// the UI; this documents the threshold and anchors the tests.
#[allow(dead_code)]
pub const SUSPICIOUS: i64 = 40;
/// Advisory score (>= this) worth a "likely scam" badge. Harm-gated.
pub const LIKELY_SCAM: i64 = 75;

/// Self-hosted employer applicant-tracking systems: weak text signals are too noisy
/// here (real companies write terse posts), so only harm triggers run.
const EMPLOYER_ATS: &[&str] = &[
    "greenhouse",
    "lever",
    "ashby",
    "workable",
    "recruitee",
    "smartrecruiters",
    "usajobs",
];

/// Aggregator feeds truncate descriptions and hide company names, so the two
/// structural checks that key on those are feed artifacts, not fraud.
const AGGREGATOR: &[&str] = &[
    "adzuna",
    "reed",
    "jooble",
    "findwork",
    "themuse",
    "jobicy",
    "remoteok",
    "remotive",
    "himalayas",
    "arbeitnow",
    "usajobs",
    "jsonld",
    // feed sources also truncate / omit company
    "weworkremotely",
    "workingnomads",
    "rss",
];

const FREE_WEBMAIL: &[&str] = &[
    "gmail.com",
    "googlemail.com",
    "yahoo.com",
    "ymail.com",
    "outlook.com",
    "hotmail.com",
    "live.com",
    "aol.com",
    "icloud.com",
    "me.com",
    "gmx.com",
    "protonmail.com",
    "proton.me",
    "yandex.com",
    "mail.ru",
    "zoho.com",
    "qq.com",
];

const BRANDS: &[&str] = &[
    "amazon",
    "walmart",
    "google",
    "meta",
    "apple",
    "microsoft",
    "coinbase",
    "netflix",
    "tesla",
    "fedex",
    "ups",
    "paypal",
    "stripe",
];

const SHORTENERS: &[&str] = &[
    "bit.ly",
    "tinyurl.com",
    "t.co",
    "goo.gl",
    "ow.ly",
    "is.gd",
    "cutt.ly",
    "rb.gy",
    "shorturl.at",
    "rebrand.ly",
];

/// The result of scoring one posting.
#[derive(Debug, Clone, Default)]
pub struct ScamResult {
    pub score: i64,
    /// The check ids that fired, for a "why flagged" UI tooltip.
    pub flags: Vec<String>,
}

impl ScamResult {
    /// Serialize the fired flags for storage in a single TEXT column.
    pub fn flags_joined(&self) -> Option<String> {
        if self.flags.is_empty() {
            None
        } else {
            Some(self.flags.join("|"))
        }
    }
}

struct Pats {
    upfront_fee: Regex,
    pii: Regex,
    mule: Regex,
    check_arrival: Regex,
    check_remit: Regex,
    task_crypto: Regex,
    reshipping: Regex,
    email: Regex,
    messaging: Regex,
    apply_verb: Regex,
    paid_period: Regex,
    no_experience: Regex,
    mlm: Regex,
    thin_after_apply: Regex,
    placeholder_company: Regex,
    teaser_link: Regex,
    ip_host: Regex,
    urgency_strong: Regex,
    urgency_weak: Regex,
}

fn pats() -> &'static Pats {
    static P: OnceLock<Pats> = OnceLock::new();
    P.get_or_init(|| Pats {
        upfront_fee: Regex::new(
            r"(?i)(registration|training|onboarding|processing|activation|application|starter[ -]?kit|administration|admin)\s+fee|(one[ -]?time|small|refundable|non-?refundable)\s+(fee|deposit|payment)\s+(to|before|required|of)|(pay|send|wire|transfer|deposit|venmo|zelle|cash\s?app)\b[^.]{0,40}\b(fee|deposit|\$\d|to (get|start|secure|begin)|before (you )?(start|begin))|(buy|purchase)\b[^.]{0,30}(starter[ -]?kit|inventory|equipment|software|license)\b[^.]{0,30}(to (start|begin|join)|from (our|the) (vendor|supplier))|secure your (spot|position|placement)[^.]{0,20}(fee|deposit|payment)",
        )
        .unwrap(),
        pii: Regex::new(
            r"(?i)(provide|send|share|enter|upload|submit|verify|fill out)\b[^.]{0,40}\b(ssn|social security|bank account|bank login|bank details|routing|driver'?s? licen[sc]e|passport|date of birth|\bdob\b)\b|\b(ssn|social security|bank account|bank login|bank details|routing|passport|driver'?s? licen[sc]e)\b[^.]{0,40}\b(to (onboard|start|begin|apply)|before (the )?interview|upon application|to receive)\b",
        )
        .unwrap(),
        mule: Regex::new(
            r"(?i)(receive|accept|collect)\b[^.]{0,40}\b(funds|payments?|money|transfers?|deposits?)\b[^.]{0,40}(forward|send|transfer|wire|remit|move)|(use your own|use a new|open (a )?(new )?(personal|bank|business)) account[^.]{0,40}(receive|process|forward|transfer)|(process|handle)\b[^.]{0,30}(payments?|transactions?|wire transfers?)\b[^.]{0,30}on behalf of|\b(money|funds?|payment|financial)\s+(mule|transfer agent|processing agent|processor)\b",
        )
        .unwrap(),
        check_arrival: Regex::new(
            r"(?i)(we'?ll|we will|company will|you'?ll) (send|mail|deposit)\b[^.]{0,30}(check|cheque|cashier'?s check|money order)|deposit (the|this|your) (check|cheque)",
        )
        .unwrap(),
        check_remit: Regex::new(
            r"(?i)(send|wire|transfer|return|remit|forward)\b[^.]{0,30}(remaining|remainder|balance|difference|excess|overpay|rest)|(keep|retain)\b[^.]{0,20}(remainder|difference|rest)\b[^.]{0,20}(pay|commission|fee)",
        )
        .unwrap(),
        task_crypto: Regex::new(
            r"(?i)(deposit|recharge|top ?up|add funds?)\b[^.]{0,40}(unlock|continue|complete|withdraw|next (set|task|level))|\b(usdt|trc-?20|tether|cryptocurrency|crypto|bitcoin|btc)\b[^.]{0,40}(deposit|recharge|to (unlock|withdraw|start|continue))|(complete|do|finish)\b[^.]{0,20}(simple |daily |easy )?tasks?\b[^.]{0,40}(commission|earn)\b[^.]{0,40}(deposit|recharge|invite code|activation code)|(optimi[sz]e|rate|like|boost|review)\s+(products?|hotels?|apps?|listings?|orders?|merchants?)\b[^.]{0,40}commission",
        )
        .unwrap(),
        reshipping: Regex::new(
            r"(?i)re-?ship(ping|per)?\b|package (forwarding|forwarder|handler|reshipper|inspection|inspector)|(receive|accept) packages?\b[^.]{0,30}(at|to) (your )?(home|address|residence)|(remove|discard|throw away)\b[^.]{0,30}(packaging|receipts?|labels?)|(re-?label|reprint)\b[^.]{0,20}(shipping )?labels?\b[^.]{0,40}(forward|send|ship)\b[^.]{0,30}(internationally|overseas|abroad)",
        )
        .unwrap(),
        email: Regex::new(r"(?i)[a-z0-9._%+-]+@([a-z0-9.-]+\.[a-z]{2,})").unwrap(),
        messaging: Regex::new(
            r"(?i)(contact|message|text|reach|add|dm|write to|interview|onboard)\b[^.]{0,30}(on|via|through|at)\s+(telegram|whatsapp|signal|wechat|viber)\b|t\.me/|wa\.me/|(telegram|whatsapp|signal)\b[^.]{0,20}(@[a-z0-9_]{4,}|\+\d[\d ()-]{6,})|(download|install)\s+(telegram|whatsapp|signal)\b[^.]{0,20}(to (apply|continue|start|chat))",
        )
        .unwrap(),
        apply_verb: Regex::new(
            r"(?i)(e-?mail|send|forward|contact|apply|reach|submit)\b[^.]{0,15}(your )?(cv|resume|application|details)?",
        )
        .unwrap(),
        paid_period: Regex::new(
            r"(?i)(earn|make|up to|guaranteed)\s*\$?\s?\d{3,4}(,?\d{3})?\s*(/|per|a|every)?\s*(day|week|wk|daily|weekly)\b",
        )
        .unwrap(),
        no_experience: Regex::new(
            r"(?i)(no|zero|without)\s+(experience|skills?|qualifications?|degree)\s+(needed|required|necessary|is required)|anyone can (do this|apply)",
        )
        .unwrap(),
        mlm: Regex::new(
            r"(?i)(recruit|bring in|sign up|refer)\b[^.]{0,30}(others|members|people|distributors|reps?)\b[^.]{0,30}(earn|commission|income|bonus|residual)|(build|grow|lead)\s+(your (own )?)?(team|downline|network)\b[^.]{0,40}(income|earn|commission|residual)|\b(multi-?level marketing|mlm|network marketing)\b|(starter|business|inventory|product)\s+(kit|pack|package)\b[^.]{0,20}(purchase|buy|invest)",
        )
        .unwrap(),
        thin_after_apply: Regex::new(
            r"(?i)(details|job description|responsibilities|more info(rmation)?)\s+(will be|to be)?\s*(provided|shared|given|discussed)\s+(after|once|upon|when)\s+(you )?(apply|contact|message|reply)",
        )
        .unwrap(),
        placeholder_company: Regex::new(
            r"(?i)^(n/?a|none|unknown|tbd|confidential|company|employer|hiring|recruiter|recruitment|agency|private|undisclosed|staffing|hr|hr dept|hr team|hr department|human resources|careers?|jobs?|talent)\s*(team|department|dept|agency)?$",
        )
        .unwrap(),
        teaser_link: Regex::new(r"(?i)(careers|apply|jobs?)\b[^.]{0,60}https?://").unwrap(),
        ip_host: Regex::new(r"^\d{1,3}(\.\d{1,3}){3}$").unwrap(),
        urgency_strong: Regex::new(
            r"(?i)no\s+interview\s+(needed|required)|(you'?re|you are) hired\b[^.]{0,40}(no interview|without an interview|on the spot)|hired (on the spot|immediately|today)\b[^.]{0,30}(congratulations|no interview)|congratulations[^.]{0,30}(you'?ve been|you have been|you are) (selected|chosen|hired)",
        )
        .unwrap(),
        urgency_weak: Regex::new(
            r"(?i)(urgent(ly)?|immediate(ly)?)\s+(hiring|hire|needed)|apply now and start today|limited\s+(spots|slots|positions)\s+(left|available)",
        )
        .unwrap(),
    })
}

/// Inputs to scoring. All text is the adapter-cleaned (HTML-stripped) form.
pub struct Posting<'a> {
    pub title: &'a str,
    pub company: &'a str,
    pub description: &'a str,
    pub salary: Option<&'a str>,
    pub url: &'a str,
    pub source_kind: &'a str,
    /// Detected language (ISO 639-1) or None; gates English-only text heuristics.
    pub lang: Option<&'a str>,
}

/// Return true if any guard phrase appears within `pad` bytes of any match of `pat`.
/// A match with NO nearby guard means the signal genuinely fires.
fn fires_unguarded(hay: &str, pat: &Regex, guards: &[&str]) -> bool {
    for m in pat.find_iter(hay) {
        let w = window(hay, m.start(), m.end(), 60);
        if !guards.iter().any(|g| w.contains(g)) {
            return true;
        }
    }
    false
}

/// A byte window around [start, end] padded by `pad`, snapped to char boundaries.
fn window(s: &str, start: usize, end: usize, pad: usize) -> &str {
    let mut lo = start.saturating_sub(pad);
    while lo > 0 && !s.is_char_boundary(lo) {
        lo -= 1;
    }
    let mut hi = (end + pad).min(s.len());
    while hi < s.len() && !s.is_char_boundary(hi) {
        hi += 1;
    }
    &s[lo..hi]
}

fn contains_any(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

/// Score a posting for scam signals. Pure and deterministic.
pub fn score(p: &Posting) -> ScamResult {
    let p_re = pats();
    let d = p.description.to_lowercase();
    let lt = p.title.to_lowercase();
    let dt = format!("{lt}\n{d}");
    let company_l = p.company.trim().to_lowercase();
    // English-only gate: applies when the language is unknown or detected English.
    let english = matches!(p.lang, None | Some("en"));
    let is_ats = EMPLOYER_ATS.contains(&p.source_kind);
    let is_aggregator = AGGREGATOR.contains(&p.source_kind);

    let mut flags: Vec<String> = Vec::new();
    let mut harm = 0i64;

    // ---- HARM triggers (run regardless of source) ----
    if fires_unguarded(
        &dt,
        &p_re.upfront_fee,
        &[
            "no fee",
            "no fees",
            "free to apply",
            "never ask",
            "never charge",
            "never require",
            "we provide",
            "equipment provided",
            "provided by us",
            "provided by the company",
            "reimburse",
            "bring your own",
            "employer provides",
            "paid training",
        ],
    ) {
        harm += 40;
        flags.push("harm-upfront-payment-fee".into());
    }
    if fires_unguarded(
        &d,
        &p_re.pii,
        &[
            "after an offer",
            "once hired",
            "after hire",
            "post-offer",
            "we never ask",
            "will never request",
            "work authorization",
        ],
    ) {
        harm += 40;
        flags.push("harm-pii-before-hire".into());
    }
    if mule_fires(&dt, &lt) {
        harm += 40;
        flags.push("harm-payment-handling-mule".into());
    }
    if p_re.check_arrival.is_match(&d) && p_re.check_remit.is_match(&d) {
        harm += 40;
        flags.push("harm-fake-check-overpayment".into());
    }
    if p_re.task_crypto.is_match(&dt) {
        harm += 40;
        flags.push("harm-task-deposit-crypto".into());
    }
    if reshipping_fires(&dt) {
        harm += 40;
        flags.push("harm-reshipping-parcel".into());
    }
    if brand_freemail_fires(&d, &company_l) {
        harm += 35;
        flags.push("harm-brand-impersonation-freemail".into());
    }
    let harm_fired = harm > 0;

    // ---- WEAK signals ----
    // Tracked separately so the structural trio can be capped.
    let mut content = 0i64; // content weak signals (+ harm counts as content too)
    let mut structural = 0i64;

    if !is_ats {
        // -- content weak signals --
        if english
            && fires_unguarded(
                &dt,
                &p_re.messaging,
                &[
                    "community",
                    "support channel",
                    "product updates",
                    "join our telegram",
                    "join our discord",
                ],
            )
        {
            content += 18;
            flags.push("weak-offplatform-messaging".into());
        }
        if english && freemail_apply_fires(&d, p.url) {
            content += 15;
            flags.push("weak-freemail-apply-contact".into());
        }
        let pay_fires = unrealistic_pay(&d, p.salary, p_re);
        if pay_fires {
            content += 15;
            flags.push("weak-unrealistic-pay-period".into());
        }
        if pay_fires && p_re.no_experience.is_match(&d) {
            content += 12;
            flags.push("weak-no-experience-high-pay".into());
        }
        if english && p_re.mlm.is_match(&dt) {
            content += 15;
            flags.push("weak-mlm-recruiting-income".into());
        }
        if let Some(pts) = urgency(&dt, p_re) {
            content += pts;
            flags.push("weak-urgency-no-interview".into());
        }
        if english && title_shouting(p.title) {
            content += 8;
            flags.push("weak-title-shouting-spam".into());
        }

        // -- structural weak signals (subject to the trio cap) --
        if english && !is_aggregator && thin_description(&d, p_re) {
            structural += 10;
            flags.push("weak-thin-vague-description".into());
        }
        if !is_aggregator && placeholder_company(&company_l, p_re) {
            structural += 10;
            flags.push("weak-generic-placeholder-company".into());
        }
        if let Some(pts) = suspicious_url(p.url, p_re) {
            structural += pts;
            flags.push("weak-apply-url-suspicious-host".into());
        }
    }

    // Structural-trio cap: structural sparseness alone must never approach the line.
    let content_fired = harm_fired || content > 0;
    if !content_fired {
        structural = structural.min(18);
    }

    let mut total = (harm + content + structural).min(100);
    // "Likely scam" must be harm-gated: a pile of weak signals can't reach it alone.
    if !harm_fired {
        total = total.min(LIKELY_SCAM - 1);
    }

    ScamResult {
        score: total,
        flags,
    }
}

/// Money-mule check with its finance-role + corporate-account guards.
fn mule_fires(dt: &str, title_l: &str) -> bool {
    let p = pats();
    if !p.mule.is_match(dt) {
        return false;
    }
    // A named office finance function legitimately handles client funds.
    let finance_role = Regex::new(
        r"(?i)(accountant|bookkeeper|accounts payable|accounts receivable|treasury|payroll|finance)",
    )
    .ok();
    if dt.contains("on behalf of our clients") || dt.contains("on behalf of clients") {
        if let Some(re) = &finance_role {
            if re.is_match(title_l) {
                return false;
            }
        }
    }
    // "use the corporate/company account" is not a personal-mule arrangement.
    if (dt.contains("corporate account") || dt.contains("company account"))
        && !dt.contains("your own")
        && !dt.contains("personal account")
        && !dt.contains("a new account")
    {
        return false;
    }
    true
}

/// Reshipping check, suppressed for a real warehouse/3PL role with a physical facility.
fn reshipping_fires(dt: &str) -> bool {
    if !pats().reshipping.is_match(dt) {
        return false;
    }
    let facility = contains_any(
        dt,
        &[
            "warehouse",
            "distribution center",
            "fulfillment center",
            "on-site",
            "on site",
            "forklift",
        ],
    );
    let from_home = contains_any(
        dt,
        &["from home", "your address", "your residence", "your home"],
    );
    // Suppress only a real facility role that never mentions working from home.
    !facility || from_home
}

/// Free-webmail address whose local-part or company impersonates a major brand.
fn brand_freemail_fires(d: &str, company_l: &str) -> bool {
    for c in pats().email.captures_iter(d) {
        let dom = c
            .get(1)
            .map(|m| m.as_str().to_lowercase())
            .unwrap_or_default();
        if !FREE_WEBMAIL.contains(&dom.as_str()) {
            continue;
        }
        let local = c
            .get(0)
            .map(|m| m.as_str().to_lowercase())
            .unwrap_or_default();
        if BRANDS
            .iter()
            .any(|b| company_l.contains(b) || local.contains(b))
        {
            return true;
        }
    }
    false
}

/// A free-webmail address sitting next to an apply/contact verb (weak: tiny employers
/// and staffing recruiters legitimately use Gmail).
fn freemail_apply_fires(d: &str, _url: &str) -> bool {
    let p = pats();
    for m in p.email.find_iter(d) {
        let dom = d[m.start()..m.end()]
            .rsplit('@')
            .next()
            .unwrap_or("")
            .to_lowercase();
        if !FREE_WEBMAIL.contains(&dom.as_str()) {
            continue;
        }
        let w = window(d, m.start(), m.end(), 60);
        if p.apply_verb.is_match(w) {
            return true;
        }
    }
    false
}

/// Implausibly high *sub-annual* pay. A plain large annual figure scores nothing.
fn unrealistic_pay(d: &str, salary: Option<&str>, p: &Pats) -> bool {
    if p.paid_period.is_match(d) {
        return true;
    }
    let Some(s) = salary.filter(|s| !s.trim().is_empty()) else {
        return false;
    };
    let st = s.to_lowercase();
    let period = Regex::new(r"(?i)\b(hour|hr|day|daily|week|wk|weekly)\b").unwrap();
    let token = if period.is_match(&st) {
        st.clone()
    } else if period.is_match(d) {
        d.to_string()
    } else {
        return false; // no sub-annual period => a large annual number is fine
    };
    let (smin, smax, _) = crate::util::parse_salary(s);
    let figure = smax.or(smin).unwrap_or(0);
    let weekly = token.contains("week") || token.contains("wk");
    let daily = token.contains("day") || token.contains("daily");
    let hourly = token.contains("hour") || token.contains("hr");
    (weekly && figure > 3000) || (daily && figure > 800) || (hourly && figure > 150)
}

/// Urgency / no-interview pressure. Strong tier scores full, soft urgency half.
fn urgency(dt: &str, p: &Pats) -> Option<i64> {
    if p.urgency_strong.is_match(dt) {
        Some(10)
    } else if p.urgency_weak.is_match(dt) {
        Some(5)
    } else {
        None
    }
}

fn thin_description(d: &str, p: &Pats) -> bool {
    // A "see our careers page" teaser is a teaser artifact, not fraud.
    if p.teaser_link.is_match(d) {
        return false;
    }
    if p.thin_after_apply.is_match(d) {
        return true;
    }
    let substantive = [
        "responsibilities",
        "requirements",
        "qualifications",
        "skills",
        "experience with",
        "you will",
        "what you",
    ];
    d.chars().count() < 250 && !contains_any(d, &substantive)
}

fn placeholder_company(company_l: &str, p: &Pats) -> bool {
    if company_l.is_empty() {
        return true;
    }
    if p.placeholder_company.is_match(company_l) {
        return true;
    }
    // Punctuation-only.
    !company_l.is_empty() && company_l.chars().all(|c| !c.is_alphanumeric())
}

/// A non-empty apply URL that is non-https, a raw IP, a known shortener, a free
/// form-builder host, or unparseable. An empty/missing URL is normal => no points.
fn suspicious_url(url: &str, p: &Pats) -> Option<i64> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return Some(12); // non-empty but unparseable
    };
    if parsed.scheme() == "http" {
        return Some(12);
    }
    let host = parsed
        .host_str()
        .unwrap_or("")
        .trim_start_matches("www.")
        .to_lowercase();
    if p.ip_host.is_match(&host) {
        return Some(12);
    }
    if SHORTENERS.contains(&host.as_str()) {
        return Some(12);
    }
    let form_hosts = [".weebly.com", ".wixsite.com", ".blogspot.com", ".glitch.me"];
    if form_hosts.iter().any(|h| host.ends_with(h)) || host == "forms.gle" {
        return Some(12);
    }
    if host == "docs.google.com" && parsed.path().contains("/forms") {
        return Some(12);
    }
    None
}

fn title_shouting(title: &str) -> bool {
    let letters: String = title.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if letters.len() >= 8 && letters == letters.to_uppercase() {
        return true;
    }
    let punct = title.chars().filter(|c| "!?$*".contains(*c)).count();
    if punct >= 3 {
        return true;
    }
    // Money / fire / check emoji.
    title.chars().any(|c| {
        matches!(
            c,
            '\u{1F4B0}' | '\u{1F911}' | '\u{1F525}' | '\u{2705}' | '\u{1F4B5}'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posting<'a>(desc: &'a str, kind: &'a str) -> Posting<'a> {
        Posting {
            title: "Customer Support Specialist",
            company: "Acme Inc",
            description: desc,
            salary: None,
            url: "https://acme.example/apply/1",
            source_kind: kind,
            lang: Some("en"),
        }
    }

    // ---- HARM triggers fire and reach at least "suspicious" ----

    #[test]
    fn upfront_fee_is_flagged() {
        let r = score(&posting(
            "Great role! Just pay a one-time registration fee of $50 to secure your spot.",
            "lever",
        ));
        assert!(r.score >= SUSPICIOUS, "score was {}", r.score);
        assert!(r.flags.contains(&"harm-upfront-payment-fee".to_string()));
    }

    #[test]
    fn upfront_fee_negated_is_not_flagged() {
        let r = score(&posting(
            "There is no fee to apply and we provide all equipment. Paid training included.",
            "lever",
        ));
        assert!(!r.flags.contains(&"harm-upfront-payment-fee".to_string()));
        assert!(r.score < SUSPICIOUS);
    }

    #[test]
    fn pre_hire_pii_is_flagged() {
        let r = score(&posting(
            "To onboard, please send your SSN and bank account details before the interview.",
            "rss",
        ));
        assert!(r.score >= SUSPICIOUS);
        assert!(r.flags.contains(&"harm-pii-before-hire".to_string()));
    }

    #[test]
    fn pii_after_offer_is_not_flagged() {
        let r = score(&posting(
            "A background check after an offer may require your driver's license; we never ask for bank details.",
            "rss",
        ));
        assert!(!r.flags.contains(&"harm-pii-before-hire".to_string()));
    }

    #[test]
    fn fake_check_overpayment_is_flagged() {
        let r = score(&posting(
            "We will mail you a check for supplies; deposit it, then wire the remaining balance back to our vendor.",
            "rss",
        ));
        assert!(r.score >= SUSPICIOUS);
        assert!(r.flags.contains(&"harm-fake-check-overpayment".to_string()));
    }

    #[test]
    fn money_mule_is_flagged_but_real_finance_role_is_not() {
        let scammy = score(&posting(
            "Receive client payments into your own account, then forward the funds via wire transfer.",
            "rss",
        ));
        assert!(scammy
            .flags
            .contains(&"harm-payment-handling-mule".to_string()));

        let legit = Posting {
            title: "Accounts Payable Accountant",
            company: "BigCorp",
            description: "Process payments on behalf of our clients using the corporate account.",
            salary: None,
            url: "https://bigcorp.example/jobs/1",
            source_kind: "greenhouse",
            lang: Some("en"),
        };
        assert!(!score(&legit)
            .flags
            .contains(&"harm-payment-handling-mule".to_string()));
    }

    #[test]
    fn task_deposit_crypto_is_flagged() {
        let r = score(&posting(
            "Complete simple daily tasks to earn commission; recharge your USDT balance to unlock the next set.",
            "rss",
        ));
        assert!(r.score >= SUSPICIOUS);
        assert!(r.flags.contains(&"harm-task-deposit-crypto".to_string()));
    }

    #[test]
    fn legit_blockchain_role_is_not_crypto_flagged() {
        let r = score(&posting(
            "We are a blockchain company building bitcoin infrastructure. You will write Rust.",
            "lever",
        ));
        assert!(!r.flags.contains(&"harm-task-deposit-crypto".to_string()));
    }

    #[test]
    fn reshipping_is_flagged_but_warehouse_is_not() {
        let scam = score(&posting(
            "Receive packages at your home, remove the packaging and labels, then forward them overseas.",
            "rss",
        ));
        assert!(scam.flags.contains(&"harm-reshipping-parcel".to_string()));

        let warehouse = score(&posting(
            "Warehouse package handler at our distribution center; operate a forklift on-site.",
            "greenhouse",
        ));
        assert!(!warehouse
            .flags
            .contains(&"harm-reshipping-parcel".to_string()));
    }

    #[test]
    fn brand_impersonation_freemail_is_flagged() {
        let p = Posting {
            title: "Remote Data Entry",
            company: "Amazon",
            description: "Apply by emailing your resume to amazon-careers@gmail.com today.",
            salary: None,
            url: "https://x.example",
            source_kind: "rss",
            lang: Some("en"),
        };
        let r = score(&p);
        assert!(r
            .flags
            .contains(&"harm-brand-impersonation-freemail".to_string()));
        assert!(r.score >= 35);
    }

    // ---- Aggregate behavior: thresholds, harm-gate, exemptions ----

    #[test]
    fn full_scam_reaches_likely_with_harm_plus_weak() {
        let p = Posting {
            title: "EARN $900 PER DAY FROM HOME!!!",
            company: "",
            description: "No experience needed! Earn $900 per day. Pay a small activation fee to start. Contact us on telegram @hire now.",
            salary: Some("$900 per day"),
            url: "http://bit.ly/x",
            source_kind: "rss",
            lang: Some("en"),
        };
        let r = score(&p);
        assert!(
            r.score >= LIKELY_SCAM,
            "score was {} flags {:?}",
            r.score,
            r.flags
        );
    }

    #[test]
    fn weak_signals_alone_cannot_reach_likely_scam() {
        // Several weak signals, but no harm trigger => clamped below likely-scam.
        let p = Posting {
            title: "URGENT HIRING NOW!!!",
            company: "",
            description: "Contact us on whatsapp to apply. Email resume to jobs@gmail.com. Limited spots left.",
            salary: None,
            url: "http://bit.ly/abc",
            source_kind: "rss",
            lang: Some("en"),
        };
        let r = score(&p);
        assert!(
            r.score < LIKELY_SCAM,
            "weak-only must stay under likely-scam, was {}",
            r.score
        );
    }

    #[test]
    fn ats_source_exempts_weak_signals() {
        // The same weak-ish text on a trusted ATS scores 0 (only harm runs there).
        let p = Posting {
            title: "URGENT HIRING NOW!!!",
            company: "",
            description: "Contact us on whatsapp to apply. Email resume to jobs@gmail.com.",
            salary: None,
            url: "http://bit.ly/abc",
            source_kind: "greenhouse",
            lang: Some("en"),
        };
        let r = score(&p);
        assert_eq!(r.score, 0, "ATS weak exemption; flags {:?}", r.flags);
    }

    // ---- The six adversarial stress-test postings all stay below "suspicious" ----

    #[test]
    fn stress_test_legit_postings_stay_clear() {
        let cases: &[Posting] = &[
            Posting {
                title: "Remote Customer Success Rep",
                company: "BrightSaaS",
                description: "Fully remote, no prior experience required, training provided. $45,000 base. You will help customers; responsibilities include onboarding and support.",
                salary: Some("$45000"),
                url: "https://boards.greenhouse.io/brightsaas/jobs/1",
                source_kind: "greenhouse",
                lang: Some("en"),
            },
            Posting {
                title: "Account Executive (Commission)",
                company: "Shield Insurance Brokerage",
                description: "Commission-only AE, uncapped earnings, be your own boss, OTE $150k. Email careers@shield-insurance.com.",
                salary: Some("$150000"),
                url: "https://shield-insurance.com/careers",
                source_kind: "jsonld",
                lang: Some("en"),
            },
            Posting {
                title: "Founding Engineer",
                company: "",
                description: "Low base plus significant equity, work from anywhere, fast-paced, wear many hats.",
                salary: None,
                url: "",
                source_kind: "rss",
                lang: Some("en"),
            },
            Posting {
                title: "Java Developer",
                company: "TalentBridge Staffing",
                description: "Staffing agency hiring a Java Developer for a Fortune 500 client. Send your resume and availability to recruiter.jane@gmail.com.",
                salary: None,
                url: "https://talentbridge.example/jobs/9",
                source_kind: "rss",
                lang: Some("en"),
            },
            Posting {
                title: "Independent Beauty Consultant",
                company: "GlowCosmetics",
                description: "Independent direct-sales consultant for a well-known cosmetics brand. Flexible hours, earn what you put in, start your own business.",
                salary: None,
                url: "https://glowcosmetics.example/join",
                source_kind: "jsonld",
                lang: Some("en"),
            },
            Posting {
                title: "Senior Site Reliability Engineer",
                company: "MegaTech",
                description: "Fully remote US. $220k-$280k. See the full job description and apply on our careers page: https://megatech.example/careers/sre",
                salary: Some("$220000-$280000"),
                url: "https://megatech.example/careers/sre",
                source_kind: "jsonld",
                lang: Some("en"),
            },
        ];
        for p in cases {
            let r = score(p);
            assert!(
                r.score < SUSPICIOUS,
                "legit posting '{}' scored {} (flags {:?})",
                p.title,
                r.score,
                r.flags
            );
        }
    }
}
