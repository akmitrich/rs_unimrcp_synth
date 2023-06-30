use std::{io::Read, mem::size_of};

pub mod uni {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(dead_code)]
    #![allow(clippy::all)]
    #![allow(rustdoc::broken_intra_doc_links)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

    pub const FALSE: apt_bool_t = 0;
    pub const TRUE: apt_bool_t = 1;
}

const SYNTH_ENGINE_TASK_NAME: &[u8; 17] = b"DemoSynth Engine\0";

pub static ENGINE_VTABLE: uni::mrcp_engine_method_vtable_t = uni::mrcp_engine_method_vtable_t {
    destroy: Some(engine_destroy),
    open: Some(engine_open),
    close: Some(engine_close),
    create_channel: Some(engine_create_channel),
};

pub const CHANNEL_VTABLE: uni::mrcp_engine_channel_method_vtable_t =
    uni::mrcp_engine_channel_method_vtable_t {
        destroy: Some(channel_destroy),
        open: Some(channel_open),
        close: Some(channel_close),
        process_request: Some(channel_process_request),
    };

pub static STREAM_VTABLE: uni::mpf_audio_stream_vtable_t = uni::mpf_audio_stream_vtable_t {
    destroy: Some(stream_destroy),
    open_rx: Some(stream_open),
    close_rx: Some(stream_close),
    read_frame: Some(stream_read),
    open_tx: None,
    close_tx: None,
    write_frame: None,
    trace: None,
};

#[repr(C)]
struct DemoSynthEngine {
    task: *mut uni::apt_consumer_task_t,
}

#[derive(Debug)]
#[repr(C)]
struct DemoSynthChannel {
    demo_engine: *mut DemoSynthEngine,
    channel: *mut uni::mrcp_engine_channel_t,
    speak_request: *mut uni::mrcp_message_t,
    stop_response: *mut uni::mrcp_message_t,
    time_to_complete: uni::apr_size_t,
    paused: uni::apt_bool_t,
    audio_file: Option<std::fs::File>,
}

#[repr(C)]
enum DemoSynthMsgType {
    DemoSynthMsgOpenChannel,
    DemoSynthMsgCloseChannel,
    DemoSynthMsgRequestProcess,
}

#[repr(C)]
struct DemoSynthMsg {
    type_: DemoSynthMsgType,
    channel: *mut uni::mrcp_engine_channel_t,
    request: *mut uni::mrcp_message_t,
}

#[no_mangle]
pub static mut mrcp_plugin_version: uni::mrcp_plugin_version_t = uni::mrcp_plugin_version_t {
    major: uni::PLUGIN_MAJOR_VERSION as i32,
    minor: uni::PLUGIN_MINOR_VERSION as i32,
    patch: uni::PLUGIN_PATCH_VERSION as i32,
    is_dev: 0,
};

#[no_mangle]
pub unsafe extern "C" fn mrcp_plugin_create(pool: *mut uni::apr_pool_t) -> *mut uni::mrcp_engine_t {
    let demo_engine = uni::apr_palloc(pool, size_of::<DemoSynthEngine>()) as *mut DemoSynthEngine;
    let msg_pool = uni::apt_task_msg_pool_create_dynamic(size_of::<DemoSynthMsg>(), pool);
    (*demo_engine).task = uni::apt_consumer_task_create(demo_engine as _, msg_pool, pool);
    if (*demo_engine).task.is_null() {
        return std::ptr::null_mut();
    }
    let task = uni::apt_consumer_task_base_get((*demo_engine).task);
    uni::apt_task_name_set(task, SYNTH_ENGINE_TASK_NAME.as_ptr() as _);
    let vtable = uni::apt_task_vtable_get(task);
    if !vtable.is_null() {
        (*vtable).process_msg = Some(demo_synth_msg_process);
    }
    dbg!(uni::mrcp_engine_create(
        uni::MRCP_SYNTHESIZER_RESOURCE as _,
        demo_engine as _,
        &ENGINE_VTABLE as _,
        pool,
    ))
}

unsafe extern "C" fn engine_destroy(engine: *mut uni::mrcp_engine_t) -> uni::apt_bool_t {
    let demo_engine = (*engine).obj as *mut DemoSynthEngine;
    eprintln!(
        "[DEMO-SYNTH] Destroy Engine {:?}. Custom engine = {:?}",
        engine, demo_engine
    );
    if !(*demo_engine).task.is_null() {
        let task = uni::apt_consumer_task_base_get((*demo_engine).task);
        let destroyed = uni::apt_task_destroy(task);
        (*demo_engine).task = std::ptr::null_mut() as _;
        eprintln!("[DEMO-SYNTH] Task {:?} destroyed = {:?}", task, destroyed);
    }
    uni::TRUE
}

unsafe extern "C" fn engine_open(engine: *mut uni::mrcp_engine_t) -> uni::apt_bool_t {
    let demo_engine = (*engine).obj as *mut DemoSynthEngine;
    eprintln!(
        "[DEMO-SYNTH] Open Engine {:?}. Custom engine = {:?}",
        engine, demo_engine
    );
    if !(*demo_engine).task.is_null() {
        let task = uni::apt_consumer_task_base_get((*demo_engine).task);
        let started = uni::apt_task_start(task);
        eprintln!("[DEMO-SYNTH] Task = {:?} started = {:?}.", task, started);
    }
    inline_mrcp_engine_open_respond(engine, uni::TRUE)
}

unsafe extern "C" fn engine_close(engine: *mut uni::mrcp_engine_t) -> uni::apt_bool_t {
    let demo_engine = (*engine).obj as *mut DemoSynthEngine;
    eprintln!(
        "[DEMO-SYNTH] Close Engine {:?}. Custom engine = {:?}",
        engine, demo_engine
    );
    if !(*demo_engine).task.is_null() {
        let task = uni::apt_consumer_task_base_get((*demo_engine).task);
        let terminated = uni::apt_task_terminate(task, uni::TRUE);
        eprintln!(
            "[DEMO-SYNTH] Task = {:?} terminated = {:?}.",
            task, terminated
        );
    }
    inline_mrcp_engine_close_respond(engine)
}

unsafe extern "C" fn engine_create_channel(
    engine: *mut uni::mrcp_engine_t,
    pool: *mut uni::apr_pool_t,
) -> *mut uni::mrcp_engine_channel_t {
    eprintln!(
        "[DEMO-SYNTH] Engine {:?} is going to create a channel",
        engine
    );
    let synth_channel =
        uni::apr_palloc(pool, size_of::<DemoSynthChannel>()) as *mut DemoSynthChannel;
    (*synth_channel).demo_engine = (*engine).obj as _;
    (*synth_channel).speak_request = std::ptr::null_mut() as _;
    (*synth_channel).stop_response = std::ptr::null_mut() as _;
    (*synth_channel).time_to_complete = 0;
    (*synth_channel).paused = uni::FALSE;
    (*synth_channel).audio_file = None;

    let capabilities = inline_mpf_source_stream_capabilities_create(pool);
    inline_mpf_codec_capabilities_add(
        &mut (*capabilities).codecs as _,
        (uni::MPF_SAMPLE_RATE_8000 | uni::MPF_SAMPLE_RATE_16000) as _,
        b"LPCM\0".as_ptr() as _,
    );

    let termination = uni::mrcp_engine_audio_termination_create(
        synth_channel as _,
        &STREAM_VTABLE as _,
        capabilities,
        pool,
    );
    (*synth_channel).channel = uni::mrcp_engine_channel_create(
        engine,
        &CHANNEL_VTABLE as _,
        synth_channel as _,
        termination,
        pool,
    );
    eprintln!(
        "[DEMO-SYNTH] Engine created channel = {:?}",
        (*synth_channel).channel
    );
    (*synth_channel).channel
}

pub unsafe extern "C" fn channel_destroy(
    channel: *mut uni::mrcp_engine_channel_t,
) -> uni::apt_bool_t {
    eprintln!("[DEMO-SYNTH] Channel {:?} destroy.", channel);
    uni::TRUE
}

pub unsafe extern "C" fn channel_open(channel: *mut uni::mrcp_engine_channel_t) -> uni::apt_bool_t {
    eprintln!("[DEMO-SYNTH] Channel {:?} open.", channel);
    demo_synth_msg_signal(
        DemoSynthMsgType::DemoSynthMsgOpenChannel,
        channel,
        std::ptr::null_mut() as _,
    )
}

unsafe extern "C" fn channel_close(channel: *mut uni::mrcp_engine_channel_t) -> uni::apt_bool_t {
    eprintln!("[DEMO-SYNTH] Channel {:?} close.", channel);
    demo_synth_msg_signal(
        DemoSynthMsgType::DemoSynthMsgCloseChannel,
        channel,
        std::ptr::null_mut() as _,
    )
}

unsafe extern "C" fn channel_process_request(
    channel: *mut uni::mrcp_engine_channel_t,
    request: *mut uni::mrcp_message_t,
) -> uni::apt_bool_t {
    eprintln!(
        "[DEMO-SYNTH] Channel {:?} process request {:?}.",
        channel, request
    );
    demo_synth_msg_signal(
        DemoSynthMsgType::DemoSynthMsgRequestProcess,
        channel,
        request,
    )
}

unsafe fn demo_synth_channel_speak(
    channel: *mut uni::mrcp_engine_channel_t,
    request: *mut uni::mrcp_message_t,
    response: *mut uni::mrcp_message_t,
) -> uni::apt_bool_t {
    let synth_channel = (*channel).method_obj as *mut DemoSynthChannel;
    let decriptor = uni::mrcp_engine_source_stream_codec_get(channel);
    if decriptor.is_null() {
        eprintln!("[DEMO-SYNTH] Failed to Get Codec Descriptor {:?}", *request);
        (*response).start_line.status_code = uni::MRCP_STATUS_CODE_METHOD_FAILED;
        return uni::FALSE;
    }
    (*synth_channel).time_to_complete = 0;
    (*synth_channel).audio_file = std::fs::File::open("/opt/unimrcp/data/demo-8kHz.pcm").ok();
    match &(*synth_channel).audio_file {
        Some(audio_file) => eprintln!("[DEMO-SYNTH] Set {:?} as Speech Source", audio_file),
        None => {
            eprintln!("[DEMO-SYNTH] No Speech Source Found");
            if inline_mrcp_generic_header_property_check(
                request,
                uni::GENERIC_HEADER_CONTENT_LENGTH as _,
            ) == uni::TRUE
            {
                let generic_header = inline_mrcp_generic_header_get(request);
                if !generic_header.is_null() {
                    (*synth_channel).time_to_complete = (*generic_header).content_length * 10;
                }
            }
        }
    }
    (*response).start_line.request_state = uni::MRCP_REQUEST_STATE_INPROGRESS;
    inline_mrcp_engine_channel_message_send(channel, response);
    (*synth_channel).speak_request = request;
    uni::TRUE
}

unsafe fn demo_synth_channel_stop(
    channel: *mut uni::mrcp_engine_channel_t,
    _request: *mut uni::mrcp_message_t,
    response: *mut uni::mrcp_message_t,
) -> uni::apt_bool_t {
    let synth_channel = (*channel).method_obj as *mut DemoSynthChannel;
    (*synth_channel).stop_response = response;
    uni::TRUE
}

unsafe fn demo_synth_channel_pause(
    channel: *mut uni::mrcp_engine_channel_t,
    _request: *mut uni::mrcp_message_t,
    response: *mut uni::mrcp_message_t,
) -> uni::apt_bool_t {
    let synth_channel = (*channel).method_obj as *mut DemoSynthChannel;
    (*synth_channel).paused = uni::TRUE;
    inline_mrcp_engine_channel_message_send(channel, response);
    uni::TRUE
}

unsafe fn demo_synth_channel_resume(
    channel: *mut uni::mrcp_engine_channel_t,
    _request: *mut uni::mrcp_message_t,
    response: *mut uni::mrcp_message_t,
) -> uni::apt_bool_t {
    let synth_channel = (*channel).method_obj as *mut DemoSynthChannel;
    (*synth_channel).paused = uni::FALSE;
    inline_mrcp_engine_channel_message_send(channel, response);
    uni::TRUE
}

unsafe fn demo_synth_channel_set_params(
    channel: *mut uni::mrcp_engine_channel_t,
    request: *mut uni::mrcp_message_t,
    response: *mut uni::mrcp_message_t,
) -> uni::apt_bool_t {
    let req_synth_header =
        inline_mrcp_resource_header_get(request) as *mut uni::mrcp_synth_header_t;
    if !req_synth_header.is_null() {
        if inline_mrcp_resource_header_property_check(
            request,
            uni::SYNTHESIZER_HEADER_VOICE_AGE as _,
        ) == uni::TRUE
        {
            eprintln!("Set Voice Age {}", (*req_synth_header).voice_param.age);
        }
        if inline_mrcp_resource_header_property_check(
            request,
            uni::SYNTHESIZER_HEADER_VOICE_NAME as _,
        ) == uni::TRUE
        {
            eprintln!(
                "Set Voice Name {:?}",
                (*req_synth_header).voice_param.name.buf
            )
        }
    }
    inline_mrcp_engine_channel_message_send(channel, response);
    uni::TRUE
}

unsafe fn demo_synth_channel_get_params(
    channel: *mut uni::mrcp_engine_channel_t,
    request: *mut uni::mrcp_message_t,
    response: *mut uni::mrcp_message_t,
) -> uni::apt_bool_t {
    let req_synth_header =
        inline_mrcp_resource_header_get(request) as *mut uni::mrcp_synth_header_t;
    if !req_synth_header.is_null() {
        let res_synth_header =
            inline_mrcp_resource_header_prepare(response) as *mut uni::mrcp_synth_header_t;
        if inline_mrcp_resource_header_property_check(
            request,
            uni::SYNTHESIZER_HEADER_VOICE_AGE as _,
        ) == uni::TRUE
        {
            (*res_synth_header).voice_param.age = 25;
            uni::mrcp_resource_header_property_add(
                response,
                uni::SYNTHESIZER_HEADER_VOICE_AGE as _,
            );
        }
        if inline_mrcp_resource_header_property_check(
            request,
            uni::SYNTHESIZER_HEADER_VOICE_NAME as _,
        ) == uni::TRUE
        {
            inline_apt_string_set(
                &mut (*res_synth_header).voice_param.name as _,
                b"David\0".as_ptr() as _,
            );
            uni::mrcp_resource_header_property_add(
                response,
                uni::SYNTHESIZER_HEADER_VOICE_NAME as _,
            );
        }
    }
    inline_mrcp_engine_channel_message_send(channel, response);
    uni::TRUE
}

unsafe fn demo_synth_channel_request_dispatch(
    channel: *mut uni::mrcp_engine_channel_t,
    request: *mut uni::mrcp_message_t,
) -> uni::apt_bool_t {
    let mut processed = uni::FALSE;
    let response = uni::mrcp_response_create(request, (*request).pool);
    match (*request).start_line.method_id as u32 {
        uni::SYNTHESIZER_SET_PARAMS => {
            processed = demo_synth_channel_set_params(channel, request, response)
        }
        uni::SYNTHESIZER_GET_PARAMS => {
            processed = demo_synth_channel_get_params(channel, request, response)
        }
        uni::SYNTHESIZER_SPEAK => processed = demo_synth_channel_speak(channel, request, response),
        uni::SYNTHESIZER_STOP => processed = demo_synth_channel_stop(channel, request, response),
        uni::SYNTHESIZER_PAUSE => processed = demo_synth_channel_pause(channel, request, response),
        uni::SYNTHESIZER_RESUME => {
            processed = demo_synth_channel_resume(channel, request, response)
        }
        uni::SYNTHESIZER_BARGE_IN_OCCURRED => {
            processed = demo_synth_channel_stop(channel, request, response)
        }
        _ => {}
    }
    if processed == uni::FALSE {
        inline_mrcp_engine_channel_message_send(channel, response);
    }
    uni::TRUE
}

pub unsafe extern "C" fn stream_destroy(_stream: *mut uni::mpf_audio_stream_t) -> uni::apt_bool_t {
    uni::TRUE
}

pub unsafe extern "C" fn stream_open(
    _stream: *mut uni::mpf_audio_stream_t,
    _codec: *mut uni::mpf_codec_t,
) -> uni::apt_bool_t {
    uni::TRUE
}

pub unsafe extern "C" fn stream_close(_stream: *mut uni::mpf_audio_stream_t) -> uni::apt_bool_t {
    uni::TRUE
}

pub unsafe extern "C" fn stream_read(
    stream: *mut uni::mpf_audio_stream_t,
    frame: *mut uni::mpf_frame_t,
) -> uni::apt_bool_t {
    let synth_channel = (*stream).obj as *mut DemoSynthChannel;
    if !(*synth_channel).stop_response.is_null() {
        inline_mrcp_engine_channel_message_send(
            (*synth_channel).channel,
            (*synth_channel).stop_response,
        );
        (*synth_channel).stop_response = std::ptr::null_mut() as _;
        (*synth_channel).speak_request = std::ptr::null_mut() as _;
        (*synth_channel).paused = uni::FALSE;
        (*synth_channel).audio_file = None;
        return uni::TRUE;
    }
    if !(*synth_channel).speak_request.is_null() && (*synth_channel).paused == uni::FALSE {
        let mut completed = uni::FALSE;
        match &mut (*synth_channel).audio_file {
            Some(audio_file) => {
                let size = (*frame).codec_frame.size;
                let buffer =
                    std::slice::from_raw_parts_mut((*frame).codec_frame.buffer as *mut u8, size);
                if let Ok(have_read) = audio_file.read(buffer) {
                    if have_read == size {
                        (*frame).type_ |= uni::MEDIA_FRAME_TYPE_AUDIO as i32;
                    } else {
                        completed = uni::TRUE;
                    }
                } else {
                    completed = uni::TRUE
                }
            }
            None => {
                if (*synth_channel).time_to_complete >= 10 {
                    libc::memset((*frame).codec_frame.buffer, 0, (*frame).codec_frame.size);
                    (*frame).type_ |= uni::MEDIA_FRAME_TYPE_AUDIO as i32;
                    (*synth_channel).time_to_complete -= 10;
                } else {
                    completed = uni::TRUE;
                }
            }
        }
        if completed == uni::TRUE {
            let message = uni::mrcp_event_create(
                (*synth_channel).speak_request,
                uni::SYNTHESIZER_SPEAK_COMPLETE as _,
                (*(*synth_channel).speak_request).pool,
            );
            if !message.is_null() {
                let synth_header =
                    inline_mrcp_resource_header_prepare(message) as *mut uni::mrcp_synth_header_t;
                if !synth_header.is_null() {
                    (*synth_header).completion_cause = uni::SYNTHESIZER_COMPLETION_CAUSE_NORMAL;
                    uni::mrcp_resource_header_property_add(
                        message,
                        uni::SYNTHESIZER_HEADER_COMPLETION_CAUSE as _,
                    );
                    (*message).start_line.request_state = uni::MRCP_REQUEST_STATE_COMPLETE;
                    (*synth_channel).speak_request = std::ptr::null_mut() as _;
                    (*synth_channel).audio_file = None;
                    inline_mrcp_engine_channel_message_send((*synth_channel).channel, message);
                }
            }
        }
    }
    uni::TRUE
}

unsafe extern "C" fn demo_synth_msg_signal(
    type_: DemoSynthMsgType,
    channel: *mut uni::mrcp_engine_channel_t,
    request: *mut uni::mrcp_message_t,
) -> uni::apt_bool_t {
    let mut status = uni::FALSE;
    let demo_channel = (*channel).method_obj as *mut DemoSynthChannel;
    let demo_engine = (*demo_channel).demo_engine;
    let task = uni::apt_consumer_task_base_get((*demo_engine).task);
    let msg = uni::apt_task_msg_get(task);
    if !msg.is_null() {
        (*msg).type_ = uni::TASK_MSG_USER as _;
        let demo_msg = (*msg).data.as_mut_ptr() as *mut DemoSynthMsg;
        (*demo_msg).type_ = type_;
        (*demo_msg).channel = channel;
        (*demo_msg).request = request;
        status = uni::apt_task_msg_signal(task, msg);
    }
    status
}

unsafe extern "C" fn demo_synth_msg_process(
    _task: *mut uni::apt_task_t,
    msg: *mut uni::apt_task_msg_t,
) -> uni::apt_bool_t {
    let demo_msg = (*msg).data.as_mut_ptr() as *mut DemoSynthMsg;
    match (*demo_msg).type_ {
        DemoSynthMsgType::DemoSynthMsgOpenChannel => {
            inline_mrcp_engine_channel_open_respond((*demo_msg).channel, uni::TRUE);
        }
        DemoSynthMsgType::DemoSynthMsgCloseChannel => {
            inline_mrcp_engine_channel_close_respond((*demo_msg).channel);
        }
        DemoSynthMsgType::DemoSynthMsgRequestProcess => {
            demo_synth_channel_request_dispatch((*demo_msg).channel, (*demo_msg).request);
        }
    }
    uni::TRUE
}

unsafe fn inline_mrcp_engine_open_respond(
    engine: *mut uni::mrcp_engine_t,
    status: uni::apt_bool_t,
) -> uni::apt_bool_t {
    (*(*engine).event_vtable).on_open.unwrap()(engine, status)
}

unsafe fn inline_mrcp_engine_close_respond(engine: *mut uni::mrcp_engine_t) -> uni::apt_bool_t {
    (*(*engine).event_vtable).on_close.unwrap()(engine)
}

unsafe fn inline_mrcp_engine_channel_open_respond(
    channel: *mut uni::mrcp_engine_channel_t,
    status: uni::apt_bool_t,
) -> uni::apt_bool_t {
    (*(*channel).event_vtable).on_open.unwrap()(channel, status)
}

unsafe fn inline_mrcp_engine_channel_close_respond(
    channel: *mut uni::mrcp_engine_channel_t,
) -> uni::apt_bool_t {
    (*(*channel).event_vtable).on_close.unwrap()(channel)
}

unsafe fn inline_mpf_source_stream_capabilities_create(
    pool: *mut uni::apr_pool_t,
) -> *mut uni::mpf_stream_capabilities_t {
    uni::mpf_stream_capabilities_create(uni::STREAM_DIRECTION_RECEIVE, pool)
}

unsafe fn inline_mpf_codec_capabilities_add(
    capabilities: *mut uni::mpf_codec_capabilities_t,
    sample_rates: std::os::raw::c_int,
    codec_name: *const i8,
) -> uni::apt_bool_t {
    let attribs = uni::apr_array_push((*capabilities).attrib_arr) as *mut uni::mpf_codec_attribs_t;
    inline_apt_string_assign(
        &mut (*attribs).name as _,
        codec_name,
        (*(*capabilities).attrib_arr).pool,
    );
    (*attribs).sample_rates = sample_rates;
    (*attribs).bits_per_sample = 0;
    // (*attribs).frame_duration = uni::CODEC_FRAME_TIME_BASE; // In version 1.8.0 was introduced 'frame_duration' codec property. 10 ms per frame was hardcoded in earlier versions
    uni::TRUE
}

unsafe fn inline_apt_string_assign(
    str: *mut uni::apt_str_t,
    src: *const i8,
    pool: *mut uni::apr_pool_t,
) {
    (*str).buf = std::ptr::null_mut() as _;
    (*str).length = if src.is_null() { 0 } else { libc::strlen(src) };
    if (*str).length > 0 {
        (*str).buf = uni::apr_pstrmemdup(pool, src, (*str).length);
    }
}

unsafe fn inline_mrcp_generic_header_property_check(
    message: *const uni::mrcp_message_t,
    id: uni::apr_size_t,
) -> uni::apt_bool_t {
    inline_apt_header_section_field_check(&(*message).header.header_section as _, id)
}

unsafe fn inline_apt_header_section_field_check(
    header: *const uni::apt_header_section_t,
    id: uni::apr_size_t,
) -> uni::apt_bool_t {
    let arr_size = (*header).arr_size;
    let arr = std::slice::from_raw_parts((*header).arr, arr_size);
    if id < arr_size {
        return if arr[id].is_null() {
            uni::FALSE
        } else {
            uni::TRUE
        };
    }
    uni::FALSE
}

unsafe fn inline_mrcp_generic_header_get(
    message: *const uni::mrcp_message_t,
) -> *mut uni::mrcp_generic_header_t {
    (*message).header.generic_header_accessor.data as _
}

unsafe fn inline_mrcp_engine_channel_message_send(
    channel: *mut uni::mrcp_engine_channel_t,
    message: *mut uni::mrcp_message_t,
) -> uni::apt_bool_t {
    (*(*channel).event_vtable).on_message.unwrap()(channel, message)
}

unsafe fn inline_mrcp_resource_header_get(
    message: *const uni::mrcp_message_t,
) -> *mut libc::c_void {
    (*message).header.resource_header_accessor.data
}

unsafe fn inline_mrcp_resource_header_property_check(
    message: *const uni::mrcp_message_t,
    id: uni::apr_size_t,
) -> uni::apt_bool_t {
    inline_apt_header_section_field_check(
        &(*message).header.header_section as _,
        id + uni::GENERIC_HEADER_COUNT as usize,
    )
}

unsafe fn inline_mrcp_resource_header_prepare(
    mrcp_message: *mut uni::mrcp_message_t,
) -> *mut libc::c_void {
    inline_mrcp_header_allocate(
        &mut (*mrcp_message).header.resource_header_accessor as _,
        (*mrcp_message).pool,
    )
}

unsafe fn inline_mrcp_header_allocate(
    accessor: *mut uni::mrcp_header_accessor_t,
    pool: *mut uni::apr_pool_t,
) -> *mut libc::c_void {
    if !(*accessor).data.is_null() {
        return (*accessor).data;
    }
    if (*accessor).vtable.is_null() || (*(*accessor).vtable).allocate.is_none() {
        return std::ptr::null_mut() as _;
    }
    (*(*accessor).vtable).allocate.unwrap()(accessor, pool)
}

unsafe fn inline_apt_string_set(str: *mut uni::apt_str_t, src: *const i8) {
    (*str).buf = src as _;
    (*str).length = if src.is_null() { 0 } else { libc::strlen(src) }
}
