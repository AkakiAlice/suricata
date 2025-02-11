/* Copyright (C) 2024 Open Information Security Foundation
 *
 * You can copy, redistribute or modify this Program under the terms of
 * the GNU General Public License version 2 as published by the Free
 * Software Foundation.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * version 2 along with this program; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA
 * 02110-1301, USA.
 */

use super::ldap::{LdapTransaction, ALPROTO_LDAP};
use crate::detect::uint::{
    detect_parse_uint_enum, rs_detect_u32_free, rs_detect_u32_match, rs_detect_u32_parse,
    rs_detect_u8_free, rs_detect_u8_match, DetectUintData,
};
use crate::detect::{
    DetectBufferSetActiveList, DetectHelperBufferRegister, DetectHelperGetMultiData,
    DetectHelperKeywordRegister, DetectHelperMultiBufferMpmRegister, DetectSignatureSetAppProto,
    SCSigTableElmt, SigMatchAppendSMToList, SIGMATCH_INFO_STICKY_BUFFER, SIGMATCH_NOOPT,
};
use crate::ldap::types::{LdapMessage, LdapResultCode, ProtocolOp, ProtocolOpCode};

use std::ffi::CStr;
use std::os::raw::{c_int, c_void};
use std::str::FromStr;

#[derive(Debug, PartialEq)]
enum LdapIndex {
    Any,
    All,
    Index(i32),
}

#[derive(Debug, PartialEq)]
struct DetectLdapRespOperationData {
    /// Ldap response operation code
    pub du8: DetectUintData<u8>,
    /// Index can be Any to match with any responses index,
    /// All to match if all indices, or an i32 integer
    /// Negative values represent back to front indexing.
    pub index: LdapIndex,
}

struct DetectLdapRespResultData {
    /// Ldap result code
    pub du32: DetectUintData<u32>,
    /// Index can be Any to match with any responses index,
    /// All to match if all indices, or an i32 integer
    /// Negative values represent back to front indexing.
    pub index: LdapIndex,
}

static mut G_LDAP_REQUEST_OPERATION_KW_ID: c_int = 0;
static mut G_LDAP_REQUEST_OPERATION_BUFFER_ID: c_int = 0;
static mut G_LDAP_RESPONSES_OPERATION_KW_ID: c_int = 0;
static mut G_LDAP_RESPONSES_OPERATION_BUFFER_ID: c_int = 0;
static mut G_LDAP_RESPONSES_COUNT_KW_ID: c_int = 0;
static mut G_LDAP_RESPONSES_COUNT_BUFFER_ID: c_int = 0;
static mut G_LDAP_RESPONSES_RESULT_CODE_KW_ID: c_int = 0;
static mut G_LDAP_RESPONSES_RESULT_CODE_BUFFER_ID: c_int = 0;
static mut G_LDAP_RESPONSES_ERROR_MSG_BUFFER_ID: c_int = 0;

unsafe extern "C" fn ldap_parse_protocol_req_op(
    ustr: *const std::os::raw::c_char,
) -> *mut DetectUintData<u8> {
    let ft_name: &CStr = CStr::from_ptr(ustr); //unsafe
    if let Ok(s) = ft_name.to_str() {
        if let Some(ctx) = detect_parse_uint_enum::<u8, ProtocolOpCode>(s) {
            let boxed = Box::new(ctx);
            return Box::into_raw(boxed) as *mut _;
        }
    }
    return std::ptr::null_mut();
}

unsafe extern "C" fn ldap_detect_request_operation_setup(
    de: *mut c_void, s: *mut c_void, raw: *const libc::c_char,
) -> c_int {
    if DetectSignatureSetAppProto(s, ALPROTO_LDAP) != 0 {
        return -1;
    }
    let ctx = ldap_parse_protocol_req_op(raw) as *mut c_void;
    if ctx.is_null() {
        return -1;
    }
    if SigMatchAppendSMToList(
        de,
        s,
        G_LDAP_REQUEST_OPERATION_KW_ID,
        ctx,
        G_LDAP_REQUEST_OPERATION_BUFFER_ID,
    )
    .is_null()
    {
        ldap_detect_request_free(std::ptr::null_mut(), ctx);
        return -1;
    }
    return 0;
}

unsafe extern "C" fn ldap_detect_request_operation_match(
    _de: *mut c_void, _f: *mut c_void, _flags: u8, _state: *mut c_void, tx: *mut c_void,
    _sig: *const c_void, ctx: *const c_void,
) -> c_int {
    let tx = cast_pointer!(tx, LdapTransaction);
    let ctx = cast_pointer!(ctx, DetectUintData<u8>);
    if let Some(request) = &tx.request {
        let option = request.protocol_op.to_u8();
        return rs_detect_u8_match(option, ctx);
    }
    return 0;
}

unsafe extern "C" fn ldap_detect_request_free(_de: *mut c_void, ctx: *mut c_void) {
    // Just unbox...
    let ctx = cast_pointer!(ctx, DetectUintData<u8>);
    rs_detect_u8_free(ctx);
}

fn aux_ldap_parse_protocol_resp_op(s: &str) -> Option<DetectLdapRespOperationData> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() > 2 {
        return None;
    }
    let index = if parts.len() == 2 {
        match parts[1] {
            "all" => LdapIndex::All,
            "any" => LdapIndex::Any,
            _ => {
                let i32_index = i32::from_str(parts[1]).ok()?;
                LdapIndex::Index(i32_index)
            }
        }
    } else {
        LdapIndex::Any
    };
    if let Some(ctx) = detect_parse_uint_enum::<u8, ProtocolOpCode>(parts[0]) {
        let du8 = ctx;
        return Some(DetectLdapRespOperationData { du8, index });
    }
    return None;
}

unsafe extern "C" fn ldap_parse_protocol_resp_op(
    ustr: *const std::os::raw::c_char,
) -> *mut DetectUintData<u8> {
    let ft_name: &CStr = CStr::from_ptr(ustr); //unsafe
    if let Ok(s) = ft_name.to_str() {
        if let Some(ctx) = aux_ldap_parse_protocol_resp_op(s) {
            let boxed = Box::new(ctx);
            return Box::into_raw(boxed) as *mut _;
        }
    }
    return std::ptr::null_mut();
}

unsafe extern "C" fn ldap_detect_responses_operation_setup(
    de: *mut c_void, s: *mut c_void, raw: *const libc::c_char,
) -> c_int {
    if DetectSignatureSetAppProto(s, ALPROTO_LDAP) != 0 {
        return -1;
    }
    let ctx = ldap_parse_protocol_resp_op(raw) as *mut c_void;
    if ctx.is_null() {
        return -1;
    }
    if SigMatchAppendSMToList(
        de,
        s,
        G_LDAP_RESPONSES_OPERATION_KW_ID,
        ctx,
        G_LDAP_RESPONSES_OPERATION_BUFFER_ID,
    )
    .is_null()
    {
        ldap_detect_responses_free(std::ptr::null_mut(), ctx);
        return -1;
    }
    return 0;
}

unsafe extern "C" fn ldap_detect_responses_operation_match(
    _de: *mut c_void, _f: *mut c_void, _flags: u8, _state: *mut c_void, tx: *mut c_void,
    _sig: *const c_void, ctx: *const c_void,
) -> c_int {
    let tx = cast_pointer!(tx, LdapTransaction);
    let ctx = cast_pointer!(ctx, DetectLdapRespOperationData);

    match ctx.index {
        LdapIndex::Any => {
            for response in &tx.responses {
                let option: u8 = response.protocol_op.to_u8();
                if rs_detect_u8_match(option, &ctx.du8) == 1 {
                    return 1;
                }
            }
            return 0;
        }
        LdapIndex::All => {
            for response in &tx.responses {
                let option: u8 = response.protocol_op.to_u8();
                if rs_detect_u8_match(option, &ctx.du8) == 0 {
                    return 0;
                }
            }
            return 1;
        }
        LdapIndex::Index(idx) => {
            let index = if idx < 0 {
                // negative values for backward indexing.
                ((tx.responses.len() as i32) + idx) as usize
            } else {
                idx as usize
            };
            if tx.responses.len() <= index {
                return 0;
            }
            let response: &LdapMessage = &tx.responses[index];
            let option: u8 = response.protocol_op.to_u8();
            return rs_detect_u8_match(option, &ctx.du8);
        }
    }
}

unsafe extern "C" fn ldap_detect_responses_free(_de: *mut c_void, ctx: *mut c_void) {
    // Just unbox...
    let ctx = cast_pointer!(ctx, DetectLdapRespOperationData);
    std::mem::drop(Box::from_raw(ctx));
}

unsafe extern "C" fn ldap_detect_responses_count_setup(
    de: *mut c_void, s: *mut c_void, raw: *const libc::c_char,
) -> c_int {
    if DetectSignatureSetAppProto(s, ALPROTO_LDAP) != 0 {
        return -1;
    }
    let ctx = rs_detect_u32_parse(raw) as *mut c_void;
    if ctx.is_null() {
        return -1;
    }
    if SigMatchAppendSMToList(
        de,
        s,
        G_LDAP_RESPONSES_COUNT_KW_ID,
        ctx,
        G_LDAP_RESPONSES_COUNT_BUFFER_ID,
    )
    .is_null()
    {
        ldap_detect_responses_count_free(std::ptr::null_mut(), ctx);
        return -1;
    }
    return 0;
}

unsafe extern "C" fn ldap_detect_responses_count_match(
    _de: *mut c_void, _f: *mut c_void, _flags: u8, _state: *mut c_void, tx: *mut c_void,
    _sig: *const c_void, ctx: *const c_void,
) -> c_int {
    let tx = cast_pointer!(tx, LdapTransaction);
    let ctx = cast_pointer!(ctx, DetectUintData<u32>);
    let len = tx.responses.len() as u32;
    return rs_detect_u32_match(len, ctx);
}

unsafe extern "C" fn ldap_detect_responses_count_free(_de: *mut c_void, ctx: *mut c_void) {
    // Just unbox...
    let ctx = cast_pointer!(ctx, DetectUintData<u32>);
    rs_detect_u32_free(ctx);
}

fn aux_ldap_parse_resp_result_code(s: &str) -> Option<DetectLdapRespResultData> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() > 2 {
        return None;
    }
    let index = if parts.len() == 2 {
        match parts[1] {
            "all" => LdapIndex::All,
            "any" => LdapIndex::Any,
            _ => {
                let i32_index = i32::from_str(parts[1]).ok()?;
                LdapIndex::Index(i32_index)
            }
        }
    } else {
        LdapIndex::Any
    };
    if let Some(ctx) = detect_parse_uint_enum::<u32, LdapResultCode>(parts[0]) {
        let du32 = ctx;
        return Some(DetectLdapRespResultData { du32, index });
    }
    return None;
}

unsafe extern "C" fn ldap_parse_responses_result_code(
    ustr: *const std::os::raw::c_char,
) -> *mut DetectUintData<u32> {
    let ft_name: &CStr = CStr::from_ptr(ustr); //unsafe
    if let Ok(s) = ft_name.to_str() {
        if let Some(ctx) = aux_ldap_parse_resp_result_code(s) {
            let boxed = Box::new(ctx);
            return Box::into_raw(boxed) as *mut _;
        }
    }
    return std::ptr::null_mut();
}

unsafe extern "C" fn ldap_detect_responses_result_code_setup(
    de: *mut c_void, s: *mut c_void, raw: *const libc::c_char,
) -> c_int {
    if DetectSignatureSetAppProto(s, ALPROTO_LDAP) != 0 {
        return -1;
    }
    let ctx = ldap_parse_responses_result_code(raw) as *mut c_void;
    if ctx.is_null() {
        return -1;
    }
    if SigMatchAppendSMToList(
        de,
        s,
        G_LDAP_RESPONSES_RESULT_CODE_KW_ID,
        ctx,
        G_LDAP_RESPONSES_RESULT_CODE_BUFFER_ID,
    )
    .is_null()
    {
        ldap_detect_responses_result_code_free(std::ptr::null_mut(), ctx);
        return -1;
    }
    return 0;
}

fn get_ldap_result_code(response: &LdapMessage) -> Option<u32> {
    let result_code = match &response.protocol_op {
        ProtocolOp::BindResponse(req) => Some(req.result.result_code.0),
        ProtocolOp::SearchResultDone(req) => Some(req.result_code.0),
        ProtocolOp::ModifyResponse(req) => Some(req.result.result_code.0),
        ProtocolOp::AddResponse(req) => Some(req.result_code.0),
        ProtocolOp::DelResponse(req) => Some(req.result_code.0),
        ProtocolOp::ModDnResponse(req) => Some(req.result_code.0),
        ProtocolOp::CompareResponse(req) => Some(req.result_code.0),
        ProtocolOp::ExtendedResponse(req) => Some(req.result.result_code.0),
        _ => None,
    };
    return result_code;
}

unsafe extern "C" fn ldap_detect_responses_result_code_match(
    _de: *mut c_void, _f: *mut c_void, _flags: u8, _state: *mut c_void, tx: *mut c_void,
    _sig: *const c_void, ctx: *const c_void,
) -> c_int {
    let tx = cast_pointer!(tx, LdapTransaction);
    let ctx = cast_pointer!(ctx, DetectLdapRespResultData);

    match ctx.index {
        LdapIndex::Any => {
            for responses in &tx.responses {
                let result_code = get_ldap_result_code(responses);
                if let Some(rc) = result_code {
                    if rs_detect_u32_match(rc, &ctx.du32) == 1 {
                        return 1;
                    }
                }
            }
            return 0;
        }
        LdapIndex::All => {
            for responses in &tx.responses {
                let result_code = get_ldap_result_code(responses);
                if let Some(rc) = result_code {
                    if rs_detect_u32_match(rc, &ctx.du32) == 0 {
                        return 0;
                    }
                }
            }
            return 1;
        }
        LdapIndex::Index(idx) => {
            let index = if idx < 0 {
                // negative values for backward indexing.
                ((tx.responses.len() as i32) + idx) as usize
            } else {
                idx as usize
            };
            if tx.responses.len() <= index {
                return 0;
            }
            let response: &LdapMessage = &tx.responses[index];
            let result_code = get_ldap_result_code(response);
            if let Some(rc) = result_code {
                return rs_detect_u32_match(rc, &ctx.du32);
            }
            return 0;
        }
    }
}

unsafe extern "C" fn ldap_detect_responses_result_code_free(_de: *mut c_void, ctx: *mut c_void) {
    // Just unbox...
    let ctx = cast_pointer!(ctx, DetectLdapRespResultData);
    std::mem::drop(Box::from_raw(ctx));
}

unsafe extern "C" fn ldap_detect_responses_error_msg_setup(
    de: *mut c_void, s: *mut c_void, _raw: *const std::os::raw::c_char,
) -> c_int {
    if DetectSignatureSetAppProto(s, ALPROTO_LDAP) != 0 {
        return -1;
    }
    if DetectBufferSetActiveList(de, s, G_LDAP_RESPONSES_ERROR_MSG_BUFFER_ID) < 0 {
        return -1;
    }
    return 0;
}

unsafe extern "C" fn ldap_detect_responses_error_msg_get_data(
    de: *mut c_void, transforms: *const c_void, flow: *const c_void, flow_flags: u8,
    tx: *const c_void, list_id: c_int, local_id: u32,
) -> *mut c_void {
    return DetectHelperGetMultiData(
        de,
        transforms,
        flow,
        flow_flags,
        tx,
        list_id,
        local_id,
        ldap_tx_get_responses_error_msg,
    );
}

unsafe extern "C" fn ldap_tx_get_responses_error_msg(
    tx: *const c_void, _flags: u8, local_id: u32, buffer: *mut *const u8, buffer_len: *mut u32,
) -> bool {
    let tx = cast_pointer!(tx, LdapTransaction);

    if local_id as usize >= tx.responses.len() {
        return false;
    }
    *buffer = std::ptr::null();
    *buffer_len = 0;

    let response = &tx.responses[local_id as usize];
    // We expect every response in one tx to be the same protocol_op
    let str_buffer: &str = match &response.protocol_op {
        ProtocolOp::BindResponse(req) => req.result.diagnostic_message.0.as_str(),
        ProtocolOp::SearchResultDone(req) => req.diagnostic_message.0.as_str(),
        ProtocolOp::ModifyResponse(req) => req.result.diagnostic_message.0.as_str(),
        ProtocolOp::AddResponse(req) => req.diagnostic_message.0.as_str(),
        ProtocolOp::DelResponse(req) => req.diagnostic_message.0.as_str(),
        ProtocolOp::ModDnResponse(req) => req.diagnostic_message.0.as_str(),
        ProtocolOp::CompareResponse(req) => req.diagnostic_message.0.as_str(),
        ProtocolOp::ExtendedResponse(req) => req.result.diagnostic_message.0.as_str(),
        _ => return false,
    };
    

    *buffer = str_buffer.as_ptr();
    *buffer_len = str_buffer.len() as u32;
    return true;
}

#[no_mangle]
pub unsafe extern "C" fn ScDetectLdapRegister() {
    let kw = SCSigTableElmt {
        name: b"ldap.request.operation\0".as_ptr() as *const libc::c_char,
        desc: b"match LDAP request operation\0".as_ptr() as *const libc::c_char,
        url: b"/rules/ldap-keywords.html#ldap.request.operation\0".as_ptr() as *const libc::c_char,
        AppLayerTxMatch: Some(ldap_detect_request_operation_match),
        Setup: ldap_detect_request_operation_setup,
        Free: Some(ldap_detect_request_free),
        flags: 0,
    };
    G_LDAP_REQUEST_OPERATION_KW_ID = DetectHelperKeywordRegister(&kw);
    G_LDAP_REQUEST_OPERATION_BUFFER_ID = DetectHelperBufferRegister(
        b"ldap.request.operation\0".as_ptr() as *const libc::c_char,
        ALPROTO_LDAP,
        false, //to client
        true,  //to server
    );
    let kw = SCSigTableElmt {
        name: b"ldap.responses.operation\0".as_ptr() as *const libc::c_char,
        desc: b"match LDAP responses operation\0".as_ptr() as *const libc::c_char,
        url: b"/rules/ldap-keywords.html#ldap.responses.operation\0".as_ptr()
            as *const libc::c_char,
        AppLayerTxMatch: Some(ldap_detect_responses_operation_match),
        Setup: ldap_detect_responses_operation_setup,
        Free: Some(ldap_detect_responses_free),
        flags: 0,
    };
    G_LDAP_RESPONSES_OPERATION_KW_ID = DetectHelperKeywordRegister(&kw);
    G_LDAP_RESPONSES_OPERATION_BUFFER_ID = DetectHelperBufferRegister(
        b"ldap.responses.operation\0".as_ptr() as *const libc::c_char,
        ALPROTO_LDAP,
        true,  //to client
        false, //to server
    );
    let kw = SCSigTableElmt {
        name: b"ldap.responses.count\0".as_ptr() as *const libc::c_char,
        desc: b"match number of LDAP responses\0".as_ptr() as *const libc::c_char,
        url: b"/rules/ldap-keywords.html#ldap.responses.count\0".as_ptr() as *const libc::c_char,
        AppLayerTxMatch: Some(ldap_detect_responses_count_match),
        Setup: ldap_detect_responses_count_setup,
        Free: Some(ldap_detect_responses_count_free),
        flags: 0,
    };
    G_LDAP_RESPONSES_COUNT_KW_ID = DetectHelperKeywordRegister(&kw);
    G_LDAP_RESPONSES_COUNT_BUFFER_ID = DetectHelperBufferRegister(
        b"ldap.responses.count\0".as_ptr() as *const libc::c_char,
        ALPROTO_LDAP,
        true,  //to client
        false, //to server
    );
    let kw = SCSigTableElmt {
        name: b"ldap.responses.result_code\0".as_ptr() as *const libc::c_char,
        desc: b"match LDAPResult code\0".as_ptr() as *const libc::c_char,
        url: b"/rules/ldap-keywords.html#ldap.responses.result_code\0".as_ptr()
            as *const libc::c_char,
        AppLayerTxMatch: Some(ldap_detect_responses_result_code_match),
        Setup: ldap_detect_responses_result_code_setup,
        Free: Some(ldap_detect_responses_result_code_free),
        flags: 0,
    };
    G_LDAP_RESPONSES_RESULT_CODE_KW_ID = DetectHelperKeywordRegister(&kw);
    G_LDAP_RESPONSES_RESULT_CODE_BUFFER_ID = DetectHelperBufferRegister(
        b"ldap.responses.result_code\0".as_ptr() as *const libc::c_char,
        ALPROTO_LDAP,
        true,  //to client
        false, //to server
    );
    let kw = SCSigTableElmt {
        name: b"ldap.responses.error_message\0".as_ptr() as *const libc::c_char,
        desc: b"match LDAPResult error message for responses\0".as_ptr() as *const libc::c_char,
        url: b"/rules/ldap-keywords.html#ldap.responses.error_message\0".as_ptr()
            as *const libc::c_char,
        Setup: ldap_detect_responses_error_msg_setup,
        flags: SIGMATCH_NOOPT | SIGMATCH_INFO_STICKY_BUFFER,
        AppLayerTxMatch: None,
        Free: None,
    };
    let _g_ldap_responses_dn_kw_id = DetectHelperKeywordRegister(&kw);
    G_LDAP_RESPONSES_ERROR_MSG_BUFFER_ID = DetectHelperMultiBufferMpmRegister(
        b"ldap.responses.error_message\0".as_ptr() as *const libc::c_char,
        b"LDAP RESPONSES DISTINGUISHED_NAME\0".as_ptr() as *const libc::c_char,
        ALPROTO_LDAP,
        true,  //to client
        false, //to server
        ldap_detect_responses_error_msg_get_data,
    );
}
