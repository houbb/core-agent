/// Question — Human-in-the-loop interaction types.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuestionType {
    /// Choose one from multiple options
    Choice,
    /// Yes/No confirmation
    Confirm,
    /// Free-form text input
    Input,
    /// Requires explicit approval for a high-risk action
    Approval,
    /// Code or plan review request
    Review,
}

impl QuestionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Choice => "CHOICE",
            Self::Confirm => "CONFIRM",
            Self::Input => "INPUT",
            Self::Approval => "APPROVAL",
            Self::Review => "REVIEW",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuestionStatus {
    Pending,
    Answered,
    TimedOut,
    Cancelled,
}

impl QuestionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Answered => "ANSWERED",
            Self::TimedOut => "TIMED_OUT",
            Self::Cancelled => "CANCELLED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Question {
    pub id: uuid::Uuid,
    /// Session this question belongs to
    pub session_id: uuid::Uuid,
    /// The question type
    pub question_type: QuestionType,
    /// The question text shown to user
    pub content: String,
    /// Available options (for Choice type)
    pub options: Vec<QuestionOption>,
    /// Whether an answer is required
    pub required: bool,
    /// Current status
    pub status: QuestionStatus,
    /// User's answer (populated after response)
    pub answer: Option<String>,
    /// Context metadata (tool name, risk level, etc.)
    pub context: serde_json::Value,
    /// When the question was created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the question was answered
    pub answered_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Question {
    pub fn new_choice(
        session_id: uuid::Uuid,
        content: impl Into<String>,
        options: Vec<QuestionOption>,
    ) -> QuestionResult<Self> {
        let q = Self {
            id: uuid::Uuid::new_v4(),
            session_id,
            question_type: QuestionType::Choice,
            content: content.into(),
            options,
            required: true,
            status: QuestionStatus::Pending,
            answer: None,
            context: serde_json::Value::Null,
            created_at: chrono::Utc::now(),
            answered_at: None,
        };
        q.validate()?;
        Ok(q)
    }

    pub fn new_confirm(
        session_id: uuid::Uuid,
        content: impl Into<String>,
    ) -> QuestionResult<Self> {
        let q = Self {
            id: uuid::Uuid::new_v4(),
            session_id,
            question_type: QuestionType::Confirm,
            content: content.into(),
            options: vec![],
            required: true,
            status: QuestionStatus::Pending,
            answer: None,
            context: serde_json::Value::Null,
            created_at: chrono::Utc::now(),
            answered_at: None,
        };
        q.validate()?;
        Ok(q)
    }

    pub fn new_input(
        session_id: uuid::Uuid,
        content: impl Into<String>,
    ) -> QuestionResult<Self> {
        let q = Self {
            id: uuid::Uuid::new_v4(),
            session_id,
            question_type: QuestionType::Input,
            content: content.into(),
            options: vec![],
            required: true,
            status: QuestionStatus::Pending,
            answer: None,
            context: serde_json::Value::Null,
            created_at: chrono::Utc::now(),
            answered_at: None,
        };
        q.validate()?;
        Ok(q)
    }

    pub fn new_approval(
        session_id: uuid::Uuid,
        content: impl Into<String>,
    ) -> QuestionResult<Self> {
        let q = Self {
            id: uuid::Uuid::new_v4(),
            session_id,
            question_type: QuestionType::Approval,
            content: content.into(),
            options: vec![],
            required: true,
            status: QuestionStatus::Pending,
            answer: None,
            context: serde_json::Value::Null,
            created_at: chrono::Utc::now(),
            answered_at: None,
        };
        q.validate()?;
        Ok(q)
    }

    pub fn validate(&self) -> Result<(), QuestionError> {
        if self.content.trim().is_empty() || self.content.len() > 4096 {
            return Err(QuestionError::Validation(
                "question content must be 1..=4096 chars".into(),
            ));
        }
        if self.question_type == QuestionType::Choice && self.options.is_empty() {
            return Err(QuestionError::Validation(
                "choice questions must have options".into(),
            ));
        }
        if self.question_type != QuestionType::Choice && !self.options.is_empty() {
            return Err(QuestionError::Validation(
                "only choice questions support options".into(),
            ));
        }
        if self.options.len() > 10 {
            return Err(QuestionError::Validation(
                "max 10 options per choice question".into(),
            ));
        }
        Ok(())
    }

    /// Answer this question
    pub fn answer(&mut self, answer: impl Into<String>) -> QuestionResult<()> {
        if self.status != QuestionStatus::Pending {
            return Err(QuestionError::InvalidState(
                "question is not pending".into(),
            ));
        }
        self.answer = Some(answer.into());
        self.status = QuestionStatus::Answered;
        self.answered_at = Some(chrono::Utc::now());
        Ok(())
    }

    /// Cancel this question
    pub fn cancel(&mut self) -> QuestionResult<()> {
        if self.status != QuestionStatus::Pending {
            return Err(QuestionError::InvalidState(
                "question is not pending".into(),
            ));
        }
        self.status = QuestionStatus::Cancelled;
        Ok(())
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum QuestionError {
    #[error("validation: {0}")]
    Validation(String),
    #[error("invalid state: {0}")]
    InvalidState(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("runtime: {0}")]
    Runtime(String),
}

pub type QuestionResult<T> = Result<T, QuestionError>;
