//! MAGIC AST. Every node carries a line number so diagnostics are useful.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    I32,
    I64,
    U8,
    F64,
    Bool,
    Str,
    Void,
    Struct(u16),
}

impl Ty {
    pub fn c_name(self) -> &'static str {
        match self {
            Ty::I32 => "int32_t",
            Ty::I64 => "int64_t",
            Ty::U8 => "uint8_t",
            Ty::F64 => "double",
            Ty::Bool => "bool",
            Ty::Str => "const char*",
            Ty::Void => "void",
            Ty::Struct(_) => "struct",
        }
    }

    pub fn is_numeric(self) -> bool {
        matches!(self, Ty::I32 | Ty::I64 | Ty::U8 | Ty::F64)
    }
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Var(String),
    Bin {
        op: BinOp,
        l: Box<Expr>,
        r: Box<Expr>,
    },
    Un {
        op: UnOp,
        e: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    Index {
        name: String,
        index: Box<Expr>,
    },
    Field {
        base: String,
        field: String,
    },
    FieldPath {
        base: String,
        path: Vec<String>,
    },
    StructLit {
        name: String,
        fields: Vec<(String, Expr)>,
    },
    ArrayLen(String),
    VecParamIndex {
        name: String,
        index: Box<Expr>,
    },
    VecParamLen(String),
    VecArgPtr(String),
    VecArgLen(String),
    VecArgCap(String),
    VecParamArgPtr(String),
    VecParamArgLen(String),
    VecParamArgCap(String),
    VecParamPop(String),
    VecParamClear(String),
    VecParamReserve {
        name: String,
        value: Box<Expr>,
    },
    IndexField {
        name: String,
        index: Box<Expr>,
        field: String,
    },
    IndexFieldPath {
        name: String,
        index: Box<Expr>,
        path: Vec<String>,
    },
    Cast {
        e: Box<Expr>,
        ty: Ty,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Let {
        name: String,
        ty: Option<Ty>,
        mutable: bool,
        init: Expr,
    },
    Assign {
        name: String,
        value: Expr,
    },
    If {
        cond: Expr,
        then: Vec<Stmt>,
        els: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Return(Option<Expr>),
    Print {
        value: Expr,
        ty: Option<Ty>,
    },
    ArrayLet {
        name: String,
        elem: Ty,
        value: Expr,
        count: Expr,
    },
    VecLet {
        name: String,
        elem: Ty,
    },
    Push {
        name: String,
        value: Expr,
    },
    VecParamPush {
        name: String,
        value: Expr,
    },
    IndexFieldAssign {
        name: String,
        index: Expr,
        field: String,
        value: Expr,
    },
    IndexFieldPathAssign {
        name: String,
        index: Expr,
        path: Vec<String>,
        value: Expr,
    },
    IndexAssign {
        name: String,
        index: Expr,
        value: Expr,
    },
    FieldAssign {
        base: String,
        field: String,
        value: Expr,
    },
    FieldPathAssign {
        base: String,
        path: Vec<String>,
        value: Expr,
    },
    Arena {
        body: Vec<Stmt>,
    },
    Assert {
        cond: Expr,
    },
    For {
        var: String,
        start: Expr,
        end: Expr,
        body: Vec<Stmt>,
        /// `par for`: iterations are independent and may run in parallel (OpenMP).
        par: bool,
    },
    Break,
    Continue,
    Expr(Expr),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamTy {
    Scalar(Ty),
    Array(Ty),
    Vec(Ty),
}

#[derive(Debug, Clone)]
pub struct Func {
    pub name: String,
    pub params: Vec<(String, ParamTy)>,
    pub ret: Ty,
    pub body: Vec<Stmt>,
    /// 1-based line in the concatenated source, used for diagnostics.
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<(String, Ty)>,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub structs: Vec<StructDef>,
    pub funcs: Vec<Func>,
    pub imports: Vec<String>,
}
