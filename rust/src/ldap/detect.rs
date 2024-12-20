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
    rs_detect_u8_free, rs_detect_u8_match, rs_detect_u8_parse, DetectUintData,
};
use crate::detect::{
    DetectHelperBufferRegister, DetectHelperKeywordRegister, DetectSignatureSetAppProto,
    SCSigTableElmt, SigMatchAppendSMToList,
};

use std::os::raw::{c_int, c_void};

static mut G_LDAP_REQUEST_OPERATION_KW_ID: c_int = 0;
static mut G_LDAP_REQUEST_OPERATION_BUFFER_ID: c_int = 0;

unsafe extern "C" fn ldap_detect_request_operation_setup(
    de: *mut c_void, s: *mut c_void, raw: *const libc::c_char,
) -> c_int {
    if DetectSignatureSetAppProto(s, ALPROTO_LDAP) != 0 {
        return -1;
    }
    let ctx = rs_detect_u8_parse(raw) as *mut c_void;
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
        ldap_detect_request_operation_free(std::ptr::null_mut(), ctx);
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
    let option: u8 = match &tx.request {
        Some(request) => request.protocol_op.to_u8(),
        None => 0,
    };
    return rs_detect_u8_match(option, ctx);
}

unsafe extern "C" fn ldap_detect_request_operation_free(_de: *mut c_void, ctx: *mut c_void) {
    // Just unbox...
    let ctx = cast_pointer!(ctx, DetectUintData<u8>);
    rs_detect_u8_free(ctx);
}

#[no_mangle]
pub unsafe extern "C" fn ScDetectLdapRegister() {
    let kw = SCSigTableElmt {
        name: b"ldap.request.operation\0".as_ptr() as *const libc::c_char,
        desc: b"match LDAP request operation\0".as_ptr() as *const libc::c_char,
        url: b"/rules/ldap-keywords.html#ldap.request.operation\0".as_ptr() as *const libc::c_char,
        AppLayerTxMatch: Some(ldap_detect_request_operation_match),
        Setup: ldap_detect_request_operation_setup,
        Free: Some(ldap_detect_request_operation_free),
        flags: 0,
    };
    G_LDAP_REQUEST_OPERATION_KW_ID = DetectHelperKeywordRegister(&kw);
    G_LDAP_REQUEST_OPERATION_BUFFER_ID = DetectHelperBufferRegister(
        b"ldap.request.operation\0".as_ptr() as *const libc::c_char,
        ALPROTO_LDAP,
        false,
        true,
    );
}
