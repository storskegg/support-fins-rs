//! Sessions across the FFI (docs/FFI.md "Sessions"): the supports a user placed by hand and the settings,
//! saved inside a 3MF this app or the site exports and restored when that file is opened again. The maths is
//! `sf_io::session` (a port of `web/ui/session.js`); this layer reads and writes the app's records.

use sf_io::json::{Json, parse};
use sf_io::session::{FormValue, Why, model_from_json, restore_session, session_of, settle};

use crate::part::{FfiError, Part, SessionNote};

/// A hand-placed support restored from a session, in the part's own frame (what `DrawKit` takes: a wall from
/// `a` to `b`, a curve also through `m`, or a sway brace stood on `face` at `a`).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SessionWall {
    pub sway: bool,
    pub a: Vec<f64>,
    pub b: Vec<f64>,
    pub m: Option<Vec<f64>>,
    pub face: u64,
    pub capped: bool,
}

/// One setting a session restores: a checkbox's `flag`, or the field's `text` (`FORM_IDS` ids, `index.html`).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SessionField {
    pub id: String,
    pub flag: Option<bool>,
    pub text: Option<String>,
}

/// What `restoreSession` asks the page to restore (`web/ui/session.js:121-158`).
#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SessionRestore {
    /// `changed` (the part read back is not the one saved) or `unreadable` (the session is malformed): nothing
    /// else is set, and the file is a plain part. `none`: this part had no session (nothing happened). `None`:
    /// restored.
    pub why: Option<String>,
    /// the hand-placed supports, in the part's own frame
    pub walls: Vec<SessionWall>,
    /// the removed fins' keys (`removedSigs`)
    pub removed: Vec<String>,
    /// the fin mode (`auto`, `full`, `draw`); `None` keeps the app's (a part added beside a plate's models)
    pub fin_mode: Option<String>,
    /// fins on or off; `None` keeps the app's
    pub fins_visible: Option<bool>,
    /// the settings, as their fields hold them once the page has settled them (`session::settle`); the ones a
    /// field cannot take are left out. Empty beside a plate.
    pub form: Vec<SessionField>,
    /// `renderImportNote` after the restore: "3MF: ...; restored 2 hand-placed supports and the settings."
    pub import_note: String,
}

#[uniffi::export]
impl Part {
    /// This part's 3MF carried a session still to be restored (`pendingSession`, `web/ui/io.js:24`).
    pub fn has_session(&self) -> bool {
        self.note().pending.is_some()
    }

    /// Restore the file's session onto this part (`restoreSession`, called by `afterLoad`, `web/ui/io.js:370-390`),
    /// once, as the part arrives; `beside`: it was added to a plate of models, whose fin settings it takes. The
    /// part's import note says what came of it. A part with no session (or one already restored) is left as it
    /// is (`afterLoad` returns early): `why` is `none`.
    pub fn restore_session(&self, beside: bool) -> Result<SessionRestore, FfiError> {
        crate::crash::guarded_value("restore session", || {
            let Some(pending) = self.note().pending.take() else {
                return SessionRestore {
                    why: Some("none".into()),
                    walls: Vec::new(),
                    removed: Vec::new(),
                    fin_mode: None,
                    fins_visible: None,
                    form: Vec::new(),
                    import_note: self.note().import_note.clone(),
                };
            };
            let outcome = restore_session(&pending, &self.topology().pos, beside);
            let mut note = self.note();
            let mut out = SessionRestore {
                why: None,
                walls: Vec::new(),
                removed: Vec::new(),
                fin_mode: None,
                fins_visible: None,
                form: Vec::new(),
                import_note: String::new(),
            };
            match outcome {
                Ok(r) => {
                    let c = r.centre;
                    let shift = |p: [f64; 3]| vec![p[0] + c[0], p[1] + c[1], p[2] + c[2]];
                    out.walls = r
                        .walls
                        .iter()
                        .map(|w| SessionWall {
                            sway: w.sway,
                            a: shift(w.a),
                            b: shift(w.b),
                            m: w.m.map(shift),
                            face: w.face.max(0) as u64,
                            capped: w.capped,
                        })
                        .collect();
                    let n = out.walls.len();
                    note.meta.session = if beside {
                        SessionNote::Beside(n)
                    } else {
                        SessionNote::Restored(n)
                    };
                    out.removed = r.removed;
                    out.fin_mode = r.fin_mode;
                    out.fins_visible = r.fins_visible;
                    out.form = r
                        .form
                        .iter()
                        .filter_map(|(id, v)| {
                            settle(id, v).map(|v| match v {
                                FormValue::Bool(b) => SessionField {
                                    id: id.clone(),
                                    flag: Some(b),
                                    text: None,
                                },
                                FormValue::Text(t) => SessionField {
                                    id: id.clone(),
                                    flag: None,
                                    text: Some(t),
                                },
                            })
                        })
                        .collect();
                }
                Err(why) => {
                    out.why = Some(why.word().to_string());
                    note.meta.session = match why {
                        Why::Changed => SessionNote::Changed,
                        Why::Unreadable => SessionNote::Unreadable,
                    };
                }
            }
            let name = self.name().to_string();
            note.render(&name);
            out.import_note = note.import_note.clone();
            out
        })
    }
}

/// `JS Number -> text` (`String(x)`): the form's text for a number setting, as the page's field would hold it.
#[uniffi::export]
pub fn js_number_text(x: f64) -> Result<String, FfiError> {
    crate::crash::guarded_value("number text", || sf_engine::jsnum::to_js_string(x))
}

/// The session `sessionOf` writes for one model: `session_json` is the app's description (see
/// `sf_io::session::model_from_json`), `rot` and `offset` the pose and seating the part is written with, and
/// `part_tris` the part in print space as written. Parsed once for the export options.
pub(crate) struct SessionInput {
    json: Json,
}

impl SessionInput {
    pub(crate) fn from_json(j: Json) -> Result<SessionInput, FfiError> {
        // check it whole now: the options are rejected before anything is written
        model_from_json(&j, [0.0; 9], [0.0; 3]).map_err(|message| FfiError::Engine { message })?;
        Ok(SessionInput { json: j })
    }

    pub(crate) fn parse(text: &str) -> Result<SessionInput, FfiError> {
        match parse(text) {
            Some(j @ Json::Obj(_)) => Self::from_json(j),
            _ => Err(FfiError::Engine {
                message: "session must be a JSON object".into(),
            }),
        }
    }

    /// The `sessionOf` of this model, posed `rot` and seated at `offset`.
    pub(crate) fn session(&self, part_tris: &[[f64; 3]], rot: [f64; 9], offset: [f64; 3]) -> Json {
        let model = model_from_json(&self.json, rot, offset).expect("checked on parse");
        session_of(part_tris, &model)
    }
}
