use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::ptr;

const RTLD_NOW: c_int = 2;
const GHOSTTY_SUCCESS: c_int = 0;
const GHOSTTY_INVALID_VALUE: c_int = -2;
const GHOSTTY_OUT_OF_SPACE: c_int = -3;
const GHOSTTY_FORMATTER_FORMAT_PLAIN: c_int = 0;
const RENDER_DATA_COLS: c_int = 1;
const RENDER_DATA_ROWS: c_int = 2;
const RENDER_DATA_DIRTY: c_int = 3;
const RENDER_DATA_ROW_ITERATOR: c_int = 4;
const RENDER_DATA_CURSOR_VISIBLE: c_int = 11;
const RENDER_DATA_CURSOR_VIEWPORT_HAS_VALUE: c_int = 14;
const RENDER_DATA_CURSOR_VIEWPORT_X: c_int = 15;
const RENDER_DATA_CURSOR_VIEWPORT_Y: c_int = 16;
const ROW_DATA_DIRTY: c_int = 1;
const ROW_DATA_CELLS: c_int = 3;
const CELL_DATA_STYLE: c_int = 2;
const CELL_DATA_BG_COLOR: c_int = 5;
const CELL_DATA_FG_COLOR: c_int = 6;
const CELL_DATA_SELECTED: c_int = 7;
const CELL_DATA_HAS_STYLING: c_int = 8;
const CELL_DATA_GRAPHEMES_UTF8: c_int = 9;

#[link(name = "dl")]
extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn dlerror() -> *const c_char;
}

type GhosttyTerminal = *mut c_void;
type GhosttyFormatter = *mut c_void;
type GhosttyRenderState = *mut c_void;
type GhosttyRenderStateRowIterator = *mut c_void;
type GhosttyRenderStateRowCells = *mut c_void;

type GhosttyTerminalNew =
    unsafe extern "C" fn(*const c_void, *mut GhosttyTerminal, GhosttyTerminalOptions) -> c_int;
type GhosttyTerminalFree = unsafe extern "C" fn(GhosttyTerminal);
type GhosttyTerminalVtWrite = unsafe extern "C" fn(GhosttyTerminal, *const u8, usize);
type GhosttyFormatterTerminalNew = unsafe extern "C" fn(
    *const c_void,
    *mut GhosttyFormatter,
    GhosttyTerminal,
    GhosttyFormatterTerminalOptions,
) -> c_int;
type GhosttyFormatterFormatAlloc =
    unsafe extern "C" fn(GhosttyFormatter, *const c_void, *mut *mut u8, *mut usize) -> c_int;
type GhosttyFormatterFree = unsafe extern "C" fn(GhosttyFormatter);
type GhosttyFree = unsafe extern "C" fn(*const c_void, *mut u8, usize);
type GhosttyRenderStateNew = unsafe extern "C" fn(*const c_void, *mut GhosttyRenderState) -> c_int;
type GhosttyRenderStateFree = unsafe extern "C" fn(GhosttyRenderState);
type GhosttyRenderStateUpdate = unsafe extern "C" fn(GhosttyRenderState, GhosttyTerminal) -> c_int;
type GhosttyRenderStateGet = unsafe extern "C" fn(GhosttyRenderState, c_int, *mut c_void) -> c_int;
type GhosttyRenderStateRowIteratorNew =
    unsafe extern "C" fn(*const c_void, *mut GhosttyRenderStateRowIterator) -> c_int;
type GhosttyRenderStateRowIteratorFree = unsafe extern "C" fn(GhosttyRenderStateRowIterator);
type GhosttyRenderStateRowIteratorNext =
    unsafe extern "C" fn(GhosttyRenderStateRowIterator) -> bool;
type GhosttyRenderStateRowGet =
    unsafe extern "C" fn(GhosttyRenderStateRowIterator, c_int, *mut c_void) -> c_int;
type GhosttyRenderStateRowCellsNew =
    unsafe extern "C" fn(*const c_void, *mut GhosttyRenderStateRowCells) -> c_int;
type GhosttyRenderStateRowCellsFree = unsafe extern "C" fn(GhosttyRenderStateRowCells);
type GhosttyRenderStateRowCellsNext = unsafe extern "C" fn(GhosttyRenderStateRowCells) -> bool;
type GhosttyRenderStateRowCellsGet =
    unsafe extern "C" fn(GhosttyRenderStateRowCells, c_int, *mut c_void) -> c_int;

#[repr(C)]
#[derive(Clone, Copy)]
struct GhosttyTerminalOptions {
    cols: u16,
    rows: u16,
    max_scrollback: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GhosttyFormatterScreenExtra {
    size: usize,
    cursor: bool,
    style: bool,
    hyperlink: bool,
    protection: bool,
    kitty_keyboard: bool,
    charsets: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GhosttyFormatterTerminalExtra {
    size: usize,
    palette: bool,
    modes: bool,
    scrolling_region: bool,
    tabstops: bool,
    pwd: bool,
    keyboard: bool,
    screen: GhosttyFormatterScreenExtra,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GhosttyFormatterTerminalOptions {
    size: usize,
    emit: c_int,
    unwrap: bool,
    trim: bool,
    extra: GhosttyFormatterTerminalExtra,
    selection: *const c_void,
}

#[repr(C)]
struct GhosttyBuffer {
    ptr: *mut u8,
    cap: usize,
    len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct GhosttyColorRgb {
    r: u8,
    g: u8,
    b: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
union GhosttyStyleColorValue {
    palette: u8,
    rgb: GhosttyColorRgb,
    _padding: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GhosttyStyleColor {
    tag: c_int,
    value: GhosttyStyleColorValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GhosttyStyle {
    size: usize,
    fg_color: GhosttyStyleColor,
    bg_color: GhosttyStyleColor,
    underline_color: GhosttyStyleColor,
    bold: bool,
    italic: bool,
    faint: bool,
    blink: bool,
    inverse: bool,
    invisible: bool,
    strikethrough: bool,
    overline: bool,
    underline: c_int,
}

#[derive(Debug, Clone)]
pub struct FormatPlainResult {
    pub text: String,
    pub library: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderSnapshot {
    pub parser: &'static str,
    pub library: String,
    pub cols: u16,
    pub rows: u16,
    pub dirty: i32,
    pub cursor: RenderCursor,
    pub row_count: usize,
    pub rows_data: Vec<RenderRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderCursor {
    pub visible: bool,
    pub in_viewport: bool,
    pub x: Option<u16>,
    pub y: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderRow {
    pub y: u16,
    pub dirty: bool,
    pub cells: Vec<RenderCell>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderCell {
    pub x: u16,
    pub text: String,
    pub style: RenderCellStyle,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct RenderRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<GhosttyColorRgb> for RenderRgb {
    fn from(value: GhosttyColorRgb) -> Self {
        Self {
            r: value.r,
            g: value.g,
            b: value.b,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct RenderCellStyle {
    pub fg: Option<RenderRgb>,
    pub bg: Option<RenderRgb>,
    pub selected: bool,
    pub has_styling: bool,
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub blink: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub overline: bool,
}

struct GhosttyVtLibrary {
    handle: *mut c_void,
    terminal_new: GhosttyTerminalNew,
    terminal_free: GhosttyTerminalFree,
    terminal_vt_write: GhosttyTerminalVtWrite,
    formatter_terminal_new: GhosttyFormatterTerminalNew,
    formatter_format_alloc: GhosttyFormatterFormatAlloc,
    formatter_free: GhosttyFormatterFree,
    ghostty_free: GhosttyFree,
    render_state_new: GhosttyRenderStateNew,
    render_state_free: GhosttyRenderStateFree,
    render_state_update: GhosttyRenderStateUpdate,
    render_state_get: GhosttyRenderStateGet,
    row_iterator_new: GhosttyRenderStateRowIteratorNew,
    row_iterator_free: GhosttyRenderStateRowIteratorFree,
    row_iterator_next: GhosttyRenderStateRowIteratorNext,
    row_get: GhosttyRenderStateRowGet,
    row_cells_new: GhosttyRenderStateRowCellsNew,
    row_cells_free: GhosttyRenderStateRowCellsFree,
    row_cells_next: GhosttyRenderStateRowCellsNext,
    row_cells_get: GhosttyRenderStateRowCellsGet,
}

impl GhosttyVtLibrary {
    unsafe fn open(path: &Path) -> Result<Self> {
        let raw_path = CString::new(path.as_os_str().as_encoded_bytes())
            .context("Ghostty VT library path contained an interior NUL")?;
        let handle = dlopen(raw_path.as_ptr(), RTLD_NOW);
        if handle.is_null() {
            return Err(anyhow!("failed to open {}: {}", path.display(), dl_error()));
        }

        let library = Self {
            handle,
            terminal_new: load_symbol(handle, "ghostty_terminal_new")?,
            terminal_free: load_symbol(handle, "ghostty_terminal_free")?,
            terminal_vt_write: load_symbol(handle, "ghostty_terminal_vt_write")?,
            formatter_terminal_new: load_symbol(handle, "ghostty_formatter_terminal_new")?,
            formatter_format_alloc: load_symbol(handle, "ghostty_formatter_format_alloc")?,
            formatter_free: load_symbol(handle, "ghostty_formatter_free")?,
            ghostty_free: load_symbol(handle, "ghostty_free")?,
            render_state_new: load_symbol(handle, "ghostty_render_state_new")?,
            render_state_free: load_symbol(handle, "ghostty_render_state_free")?,
            render_state_update: load_symbol(handle, "ghostty_render_state_update")?,
            render_state_get: load_symbol(handle, "ghostty_render_state_get")?,
            row_iterator_new: load_symbol(handle, "ghostty_render_state_row_iterator_new")?,
            row_iterator_free: load_symbol(handle, "ghostty_render_state_row_iterator_free")?,
            row_iterator_next: load_symbol(handle, "ghostty_render_state_row_iterator_next")?,
            row_get: load_symbol(handle, "ghostty_render_state_row_get")?,
            row_cells_new: load_symbol(handle, "ghostty_render_state_row_cells_new")?,
            row_cells_free: load_symbol(handle, "ghostty_render_state_row_cells_free")?,
            row_cells_next: load_symbol(handle, "ghostty_render_state_row_cells_next")?,
            row_cells_get: load_symbol(handle, "ghostty_render_state_row_cells_get")?,
        };
        Ok(library)
    }

    fn create_terminal(&self, input: &[u8], cols: u16, rows: u16) -> Result<TerminalGuard> {
        let mut terminal: GhosttyTerminal = ptr::null_mut();
        let terminal_options = GhosttyTerminalOptions {
            cols: cols.max(1),
            rows: rows.max(1),
            max_scrollback: 10_000,
        };
        let result = unsafe { (self.terminal_new)(ptr::null(), &mut terminal, terminal_options) };
        ensure_success(result, "ghostty_terminal_new")?;
        let terminal_guard = TerminalGuard {
            terminal,
            free: self.terminal_free,
        };

        if !input.is_empty() {
            unsafe {
                (self.terminal_vt_write)(terminal_guard.terminal, input.as_ptr(), input.len())
            };
        }
        Ok(terminal_guard)
    }

    fn format_plain(&self, input: &[u8], cols: u16, rows: u16) -> Result<String> {
        let terminal_guard = self.create_terminal(input, cols, rows)?;
        let mut formatter: GhosttyFormatter = ptr::null_mut();
        let formatter_options = plain_formatter_options();
        let result = unsafe {
            (self.formatter_terminal_new)(
                ptr::null(),
                &mut formatter,
                terminal_guard.terminal,
                formatter_options,
            )
        };
        ensure_success(result, "ghostty_formatter_terminal_new")?;
        let formatter_guard = FormatterGuard {
            formatter,
            free: self.formatter_free,
        };

        let mut output: *mut u8 = ptr::null_mut();
        let mut output_len: usize = 0;
        let result = unsafe {
            (self.formatter_format_alloc)(
                formatter_guard.formatter,
                ptr::null(),
                &mut output,
                &mut output_len,
            )
        };
        ensure_success(result, "ghostty_formatter_format_alloc")?;
        let allocation_guard = AllocationGuard {
            ptr: output,
            len: output_len,
            free: self.ghostty_free,
        };
        let bytes = if allocation_guard.ptr.is_null() || allocation_guard.len == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(allocation_guard.ptr, allocation_guard.len) }
        };
        Ok(String::from_utf8_lossy(bytes).to_string())
    }

    fn render_snapshot(
        &self,
        input: &[u8],
        cols: u16,
        rows: u16,
        library_path: &Path,
    ) -> Result<RenderSnapshot> {
        let terminal_guard = self.create_terminal(input, cols, rows)?;
        let mut render_state: GhosttyRenderState = ptr::null_mut();
        let result = unsafe { (self.render_state_new)(ptr::null(), &mut render_state) };
        ensure_success(result, "ghostty_render_state_new")?;
        let render_guard = RenderStateGuard {
            state: render_state,
            free: self.render_state_free,
        };

        let result =
            unsafe { (self.render_state_update)(render_guard.state, terminal_guard.terminal) };
        ensure_success(result, "ghostty_render_state_update")?;

        let snapshot_cols = self.render_get_u16(render_guard.state, RENDER_DATA_COLS)?;
        let snapshot_rows = self.render_get_u16(render_guard.state, RENDER_DATA_ROWS)?;
        let dirty = self.render_get_i32(render_guard.state, RENDER_DATA_DIRTY)?;
        let cursor = RenderCursor {
            visible: self.render_get_bool(render_guard.state, RENDER_DATA_CURSOR_VISIBLE)?,
            in_viewport: self
                .render_get_bool(render_guard.state, RENDER_DATA_CURSOR_VIEWPORT_HAS_VALUE)?,
            x: self
                .render_get_bool(render_guard.state, RENDER_DATA_CURSOR_VIEWPORT_HAS_VALUE)?
                .then(|| self.render_get_u16(render_guard.state, RENDER_DATA_CURSOR_VIEWPORT_X))
                .transpose()?,
            y: self
                .render_get_bool(render_guard.state, RENDER_DATA_CURSOR_VIEWPORT_HAS_VALUE)?
                .then(|| self.render_get_u16(render_guard.state, RENDER_DATA_CURSOR_VIEWPORT_Y))
                .transpose()?,
        };

        let mut row_iterator: GhosttyRenderStateRowIterator = ptr::null_mut();
        let result = unsafe { (self.row_iterator_new)(ptr::null(), &mut row_iterator) };
        ensure_success(result, "ghostty_render_state_row_iterator_new")?;
        let row_iterator_guard = RowIteratorGuard {
            iterator: row_iterator,
            free: self.row_iterator_free,
        };
        let mut iterator_handle = row_iterator_guard.iterator;
        let result = unsafe {
            (self.render_state_get)(
                render_guard.state,
                RENDER_DATA_ROW_ITERATOR,
                (&mut iterator_handle as *mut GhosttyRenderStateRowIterator).cast(),
            )
        };
        ensure_success(result, "ghostty_render_state_get(ROW_ITERATOR)")?;

        let mut rows_data = Vec::new();
        let mut y = 0_u16;
        while unsafe { (self.row_iterator_next)(row_iterator_guard.iterator) } {
            let dirty = self.row_get_bool(row_iterator_guard.iterator, ROW_DATA_DIRTY)?;
            let mut cells_handle: GhosttyRenderStateRowCells = ptr::null_mut();
            let result = unsafe { (self.row_cells_new)(ptr::null(), &mut cells_handle) };
            ensure_success(result, "ghostty_render_state_row_cells_new")?;
            let cells_guard = RowCellsGuard {
                cells: cells_handle,
                free: self.row_cells_free,
            };
            let mut row_cells_handle = cells_guard.cells;
            let result = unsafe {
                (self.row_get)(
                    row_iterator_guard.iterator,
                    ROW_DATA_CELLS,
                    (&mut row_cells_handle as *mut GhosttyRenderStateRowCells).cast(),
                )
            };
            ensure_success(result, "ghostty_render_state_row_get(CELLS)")?;

            let mut cells = Vec::new();
            let mut x = 0_u16;
            while unsafe { (self.row_cells_next)(cells_guard.cells) } {
                let text = self.cell_utf8(cells_guard.cells)?;
                if !text.is_empty() {
                    let style = self.cell_style(cells_guard.cells)?;
                    cells.push(RenderCell { x, text, style });
                }
                x = x.saturating_add(1);
            }
            rows_data.push(RenderRow { y, dirty, cells });
            y = y.saturating_add(1);
        }

        Ok(RenderSnapshot {
            parser: "ghostty-vt",
            library: library_path.display().to_string(),
            cols: snapshot_cols,
            rows: snapshot_rows,
            dirty,
            cursor,
            row_count: rows_data.len(),
            rows_data,
        })
    }

    fn render_get_bool(&self, state: GhosttyRenderState, data: c_int) -> Result<bool> {
        let mut out = false;
        let result =
            unsafe { (self.render_state_get)(state, data, (&mut out as *mut bool).cast()) };
        ensure_success(result, "ghostty_render_state_get(bool)")?;
        Ok(out)
    }

    fn render_get_i32(&self, state: GhosttyRenderState, data: c_int) -> Result<i32> {
        let mut out = 0_i32;
        let result = unsafe { (self.render_state_get)(state, data, (&mut out as *mut i32).cast()) };
        ensure_success(result, "ghostty_render_state_get(i32)")?;
        Ok(out)
    }

    fn render_get_u16(&self, state: GhosttyRenderState, data: c_int) -> Result<u16> {
        let mut out = 0_u16;
        let result = unsafe { (self.render_state_get)(state, data, (&mut out as *mut u16).cast()) };
        ensure_success(result, "ghostty_render_state_get(u16)")?;
        Ok(out)
    }

    fn row_get_bool(&self, iterator: GhosttyRenderStateRowIterator, data: c_int) -> Result<bool> {
        let mut out = false;
        let result = unsafe { (self.row_get)(iterator, data, (&mut out as *mut bool).cast()) };
        ensure_success(result, "ghostty_render_state_row_get(bool)")?;
        Ok(out)
    }

    fn cell_utf8(&self, cells: GhosttyRenderStateRowCells) -> Result<String> {
        let mut query = GhosttyBuffer {
            ptr: ptr::null_mut(),
            cap: 0,
            len: 0,
        };
        let result = unsafe {
            (self.row_cells_get)(
                cells,
                CELL_DATA_GRAPHEMES_UTF8,
                (&mut query as *mut GhosttyBuffer).cast(),
            )
        };
        if result == GHOSTTY_SUCCESS && query.len == 0 {
            return Ok(String::new());
        }
        if result != GHOSTTY_OUT_OF_SPACE && result != GHOSTTY_SUCCESS {
            ensure_success(result, "ghostty_render_state_row_cells_get(GRAPHEMES_UTF8)")?;
        }
        if query.len == 0 {
            return Ok(String::new());
        }
        let mut bytes = vec![0_u8; query.len];
        let mut output = GhosttyBuffer {
            ptr: bytes.as_mut_ptr(),
            cap: bytes.len(),
            len: 0,
        };
        let result = unsafe {
            (self.row_cells_get)(
                cells,
                CELL_DATA_GRAPHEMES_UTF8,
                (&mut output as *mut GhosttyBuffer).cast(),
            )
        };
        ensure_success(result, "ghostty_render_state_row_cells_get(GRAPHEMES_UTF8)")?;
        bytes.truncate(output.len);
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    fn cell_style(&self, cells: GhosttyRenderStateRowCells) -> Result<RenderCellStyle> {
        let mut style = RenderCellStyle {
            fg: self.cell_rgb(cells, CELL_DATA_FG_COLOR, "FG_COLOR")?,
            bg: self.cell_rgb(cells, CELL_DATA_BG_COLOR, "BG_COLOR")?,
            selected: self.cell_bool(cells, CELL_DATA_SELECTED, "SELECTED")?,
            has_styling: self.cell_bool(cells, CELL_DATA_HAS_STYLING, "HAS_STYLING")?,
            ..RenderCellStyle::default()
        };

        if style.has_styling {
            let ghostty_style = self.cell_ghostty_style(cells)?;
            style.bold = ghostty_style.bold;
            style.italic = ghostty_style.italic;
            style.faint = ghostty_style.faint;
            style.blink = ghostty_style.blink;
            style.inverse = ghostty_style.inverse;
            style.invisible = ghostty_style.invisible;
            style.underline = ghostty_style.underline != 0;
            style.strikethrough = ghostty_style.strikethrough;
            style.overline = ghostty_style.overline;
        }

        Ok(style)
    }

    fn cell_rgb(
        &self,
        cells: GhosttyRenderStateRowCells,
        data: c_int,
        label: &str,
    ) -> Result<Option<RenderRgb>> {
        let mut out = GhosttyColorRgb { r: 0, g: 0, b: 0 };
        let result =
            unsafe { (self.row_cells_get)(cells, data, (&mut out as *mut GhosttyColorRgb).cast()) };
        if result == GHOSTTY_INVALID_VALUE {
            return Ok(None);
        }
        ensure_success(
            result,
            &format!("ghostty_render_state_row_cells_get({label})"),
        )?;
        Ok(Some(out.into()))
    }

    fn cell_bool(
        &self,
        cells: GhosttyRenderStateRowCells,
        data: c_int,
        label: &str,
    ) -> Result<bool> {
        let mut out = false;
        let result = unsafe { (self.row_cells_get)(cells, data, (&mut out as *mut bool).cast()) };
        ensure_success(
            result,
            &format!("ghostty_render_state_row_cells_get({label})"),
        )?;
        Ok(out)
    }

    fn cell_ghostty_style(&self, cells: GhosttyRenderStateRowCells) -> Result<GhosttyStyle> {
        let mut out = default_ghostty_style();
        let result = unsafe {
            (self.row_cells_get)(
                cells,
                CELL_DATA_STYLE,
                (&mut out as *mut GhosttyStyle).cast(),
            )
        };
        ensure_success(result, "ghostty_render_state_row_cells_get(STYLE)")?;
        Ok(out)
    }
}

impl Drop for GhosttyVtLibrary {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                dlclose(self.handle);
            }
        }
    }
}

struct TerminalGuard {
    terminal: GhosttyTerminal,
    free: GhosttyTerminalFree,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        unsafe {
            (self.free)(self.terminal);
        }
    }
}

struct FormatterGuard {
    formatter: GhosttyFormatter,
    free: GhosttyFormatterFree,
}

impl Drop for FormatterGuard {
    fn drop(&mut self) {
        unsafe {
            (self.free)(self.formatter);
        }
    }
}

struct RenderStateGuard {
    state: GhosttyRenderState,
    free: GhosttyRenderStateFree,
}

impl Drop for RenderStateGuard {
    fn drop(&mut self) {
        unsafe {
            (self.free)(self.state);
        }
    }
}

struct RowIteratorGuard {
    iterator: GhosttyRenderStateRowIterator,
    free: GhosttyRenderStateRowIteratorFree,
}

impl Drop for RowIteratorGuard {
    fn drop(&mut self) {
        unsafe {
            (self.free)(self.iterator);
        }
    }
}

struct RowCellsGuard {
    cells: GhosttyRenderStateRowCells,
    free: GhosttyRenderStateRowCellsFree,
}

impl Drop for RowCellsGuard {
    fn drop(&mut self) {
        unsafe {
            (self.free)(self.cells);
        }
    }
}

struct AllocationGuard {
    ptr: *mut u8,
    len: usize,
    free: GhosttyFree,
}

impl Drop for AllocationGuard {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                (self.free)(ptr::null(), self.ptr, self.len);
            }
        }
    }
}

pub fn format_plain(input: &[u8], cols: u16, rows: u16) -> Result<FormatPlainResult> {
    let library_path = discover_library().ok_or_else(|| {
        anyhow!("libghostty-vt was not found; set CMUX_GHOSTTY_VT_LIBRARY or run `zig build -Demit-lib-vt=true` in the Ghostty checkout")
    })?;
    let library = unsafe { GhosttyVtLibrary::open(&library_path)? };
    let text = library.format_plain(input, cols, rows)?;
    Ok(FormatPlainResult {
        text,
        library: library_path,
    })
}

pub fn render_snapshot(input: &[u8], cols: u16, rows: u16) -> Result<RenderSnapshot> {
    let library_path = discover_library().ok_or_else(|| {
        anyhow!("libghostty-vt was not found; set CMUX_GHOSTTY_VT_LIBRARY or run `zig build -Demit-lib-vt=true` in the Ghostty checkout")
    })?;
    let library = unsafe { GhosttyVtLibrary::open(&library_path)? };
    library.render_snapshot(input, cols, rows, &library_path)
}

fn plain_formatter_options() -> GhosttyFormatterTerminalOptions {
    GhosttyFormatterTerminalOptions {
        size: std::mem::size_of::<GhosttyFormatterTerminalOptions>(),
        emit: GHOSTTY_FORMATTER_FORMAT_PLAIN,
        unwrap: false,
        trim: true,
        extra: GhosttyFormatterTerminalExtra {
            size: std::mem::size_of::<GhosttyFormatterTerminalExtra>(),
            palette: false,
            modes: false,
            scrolling_region: false,
            tabstops: false,
            pwd: false,
            keyboard: false,
            screen: GhosttyFormatterScreenExtra {
                size: std::mem::size_of::<GhosttyFormatterScreenExtra>(),
                cursor: false,
                style: false,
                hyperlink: false,
                protection: false,
                kitty_keyboard: false,
                charsets: false,
            },
        },
        selection: ptr::null(),
    }
}

fn default_ghostty_style() -> GhosttyStyle {
    GhosttyStyle {
        size: std::mem::size_of::<GhosttyStyle>(),
        fg_color: default_ghostty_style_color(),
        bg_color: default_ghostty_style_color(),
        underline_color: default_ghostty_style_color(),
        bold: false,
        italic: false,
        faint: false,
        blink: false,
        inverse: false,
        invisible: false,
        strikethrough: false,
        overline: false,
        underline: 0,
    }
}

fn default_ghostty_style_color() -> GhosttyStyleColor {
    GhosttyStyleColor {
        tag: 0,
        value: GhosttyStyleColorValue { _padding: 0 },
    }
}

fn ensure_success(result: c_int, function_name: &str) -> Result<()> {
    if result == GHOSTTY_SUCCESS {
        Ok(())
    } else {
        Err(anyhow!(
            "{function_name} failed with GhosttyResult {result}"
        ))
    }
}

unsafe fn load_symbol<T: Copy>(handle: *mut c_void, name: &str) -> Result<T> {
    let raw_name = CString::new(name).expect("symbol names are static");
    let symbol = dlsym(handle, raw_name.as_ptr());
    if symbol.is_null() {
        return Err(anyhow!("missing symbol {name}: {}", dl_error()));
    }
    Ok(std::mem::transmute_copy(&symbol))
}

fn dl_error() -> String {
    let error = unsafe { dlerror() };
    if error.is_null() {
        "unknown dynamic loader error".to_string()
    } else {
        unsafe { CStr::from_ptr(error) }
            .to_string_lossy()
            .to_string()
    }
}

fn discover_library() -> Option<PathBuf> {
    if let Some(path) = normalized_env_path("CMUX_GHOSTTY_VT_LIBRARY") {
        if path.exists() {
            return Some(path);
        }
    }
    let root = ghostty_root()?;
    ghostty_vt_library(&root)
}

fn ghostty_root() -> Option<PathBuf> {
    if let Some(root) = normalized_env_path("CMUX_GHOSTTY_ROOT") {
        if root.exists() {
            return Some(root);
        }
    }

    for key in ["CMUX_GHOSTTY_VT_LIBRARY", "CMUX_GHOSTTY_LIBRARY"] {
        if let Some(path) = normalized_env_path(key) {
            if let Some(root) = ghostty_root_from_library_path(&path) {
                return Some(root);
            }
        }
    }

    let mut current = std::env::current_dir().ok()?;
    loop {
        let candidate = current.join("ghostty");
        if candidate.join("include/ghostty/vt.h").exists()
            || candidate.join("zig-out/include/ghostty/vt.h").exists()
        {
            return Some(candidate);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn ghostty_vt_library(root: &Path) -> Option<PathBuf> {
    let direct = [
        root.join("zig-out/lib/libghostty-vt.so"),
        root.join("lib/libghostty-vt.so"),
    ]
    .into_iter()
    .find(|path| path.exists());
    if direct.is_some() {
        return direct;
    }
    [root.join("zig-out/lib"), root.join("lib")]
        .into_iter()
        .find_map(find_versioned_library)
}

fn ghostty_root_from_library_path(path: &Path) -> Option<PathBuf> {
    let root = path.parent()?.parent()?;
    if root.join("include/ghostty/vt.h").exists()
        || root.join("zig-out/include/ghostty/vt.h").exists()
    {
        Some(root.to_path_buf())
    } else {
        None
    }
}

fn find_versioned_library(dir: PathBuf) -> Option<PathBuf> {
    let mut matches = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("libghostty-vt.so."))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches.pop()
}

fn normalized_env_path(key: &str) -> Option<PathBuf> {
    let value = std::env::var(key).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_versioned_library_from_zig_out() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lib_dir = dir.path().join("zig-out/lib");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(lib_dir.join("libghostty-vt.so.0.1.0"), "").expect("so");

        assert_eq!(
            ghostty_vt_library(dir.path()).as_deref(),
            Some(lib_dir.join("libghostty-vt.so.0.1.0").as_path())
        );
    }

    #[test]
    fn discover_versioned_library_from_installed_prefix() {
        let dir = tempfile::tempdir().expect("tempdir");
        let lib_dir = dir.path().join("lib");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(lib_dir.join("libghostty-vt.so.0.1.0"), "").expect("so");

        assert_eq!(
            ghostty_vt_library(dir.path()).as_deref(),
            Some(lib_dir.join("libghostty-vt.so.0.1.0").as_path())
        );
    }

    #[test]
    fn infer_installed_root_from_vt_library_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let include_dir = dir.path().join("include/ghostty");
        let lib_dir = dir.path().join("lib");
        std::fs::create_dir_all(&include_dir).expect("include dir");
        std::fs::create_dir_all(&lib_dir).expect("lib dir");
        std::fs::write(include_dir.join("vt.h"), "/* vt */").expect("vt header");
        let library = lib_dir.join("libghostty-vt.so.0.1.0");
        std::fs::write(&library, "").expect("so");

        assert_eq!(
            ghostty_root_from_library_path(&library).as_deref(),
            Some(dir.path())
        );
    }

    #[test]
    fn plain_formatter_options_match_c_sized_struct_pattern() {
        let options = plain_formatter_options();
        assert_eq!(
            options.size,
            std::mem::size_of::<GhosttyFormatterTerminalOptions>()
        );
        assert_eq!(
            options.extra.size,
            std::mem::size_of::<GhosttyFormatterTerminalExtra>()
        );
        assert_eq!(
            options.extra.screen.size,
            std::mem::size_of::<GhosttyFormatterScreenExtra>()
        );
        assert_eq!(options.emit, GHOSTTY_FORMATTER_FORMAT_PLAIN);
        assert!(options.trim);
    }

    #[test]
    fn format_plain_with_real_library_when_available() {
        if discover_library().is_none() {
            eprintln!("SKIP: libghostty-vt is not available");
            return;
        }

        let result = format_plain(b"old value\r\x1b[2Knew value\n", 80, 5)
            .expect("format with libghostty-vt");
        assert!(
            result.text.contains("new value"),
            "formatted text was {:?}",
            result.text
        );
        assert!(
            !result.text.contains("old value"),
            "formatted text was {:?}",
            result.text
        );
    }

    #[test]
    fn render_snapshot_with_real_library_when_available() {
        if discover_library().is_none() {
            eprintln!("SKIP: libghostty-vt is not available");
            return;
        }

        let snapshot = render_snapshot(b"Hello\r\n\x1b[2;3HZ", 10, 3)
            .expect("render snapshot with libghostty-vt");
        assert_eq!(snapshot.cols, 10);
        assert_eq!(snapshot.rows, 3);
        assert_eq!(snapshot.row_count, 3);
        assert!(snapshot.cursor.in_viewport);
        let text = snapshot
            .rows_data
            .iter()
            .flat_map(|row| row.cells.iter())
            .map(|cell| cell.text.as_str())
            .collect::<String>();
        assert!(text.contains("Hello"), "snapshot was {snapshot:?}");
        assert!(text.contains('Z'), "snapshot was {snapshot:?}");
    }

    #[test]
    fn render_snapshot_exposes_styled_cells_when_available() {
        if discover_library().is_none() {
            eprintln!("SKIP: libghostty-vt is not available");
            return;
        }

        let snapshot = render_snapshot(b"\x1b[1;3;4;9;38;2;1;2;3;48;2;4;5;6mS\x1b[0mN", 10, 2)
            .expect("render styled snapshot with libghostty-vt");
        let styled = snapshot
            .rows_data
            .iter()
            .flat_map(|row| row.cells.iter())
            .find(|cell| cell.text == "S")
            .expect("styled cell");
        assert_eq!(styled.style.fg, Some(RenderRgb { r: 1, g: 2, b: 3 }));
        assert_eq!(styled.style.bg, Some(RenderRgb { r: 4, g: 5, b: 6 }));
        assert!(styled.style.has_styling);
        assert!(styled.style.bold);
        assert!(styled.style.italic);
        assert!(styled.style.underline);
        assert!(styled.style.strikethrough);

        let normal = snapshot
            .rows_data
            .iter()
            .flat_map(|row| row.cells.iter())
            .find(|cell| cell.text == "N")
            .expect("normal cell");
        assert_eq!(normal.style, RenderCellStyle::default());
    }
}
