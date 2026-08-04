#![cfg(feature = "jvm")]

use crate::EhError;
use crate::parser::api::parse_vote_tag;
use crate::parser::archive::{parse_archive_url, parse_archives, parse_archives_with_funds};
use crate::parser::config::parse_fav_cat;
use crate::parser::detail::{parse_comments, parse_event_pane, parse_gallery_detail};
use crate::parser::detail::{parse_pages, parse_preview_list};
use crate::parser::fav::parse_fav;
use crate::parser::home::parse_limit;
use crate::parser::list::parse_info_list;
use crate::parser::profile::{parse_profile, parse_profile_url};
use crate::parser::torrent::parse_torrent_list;
use android_logger::Config;
use anyhow::{Context, Result, ensure};
pub(super) use eh_ffi::jvm::{deref_mut_direct_bytebuffer, jni_throwing};
use eh_ffi::jvm::{input_body, serialize_to_buffer};
use jni::objects::{JByteBuffer, JClass};
use jni::sys::{JNI_TRUE, JNI_VERSION_1_6, jboolean, jint};
use jni::{JNIEnv, JavaVM};
use jni_fn::jni_fn;
use log::LevelFilter;
use serde::Serialize;
use std::ffi::c_void;
use tl::{ParserOptions, VDom};

#[jni_fn("com.hippo.ehviewer.client.parser.FavoritesParserKt")]
pub fn parseFav(mut env: JNIEnv, _class: JClass, input: JByteBuffer, limit: jint) -> jint {
    parse_marshal_inplace(&mut env, input, limit, |dom, _| {
        parse_fav(dom, dom.parser())
    })
}

#[jni_fn("com.hippo.ehviewer.client.parser.HomeParserKt")]
pub fn parseLimit(mut env: JNIEnv, _class: JClass, input: JByteBuffer, limit: jint) -> jint {
    parse_marshal_inplace(&mut env, input, limit, |dom, _| {
        parse_limit(dom, dom.parser()).context("Can't parse Limit")
    })
}

#[jni_fn("com.hippo.ehviewer.client.parser.GalleryListParserKt")]
pub fn parseGalleryInfoList(mut env: JNIEnv, _: JClass, buffer: JByteBuffer, limit: jint) -> jint {
    parse_marshal_inplace(&mut env, buffer, limit, |dom, _| {
        parse_info_list(dom, dom.parser())
    })
}

#[jni_fn("com.hippo.ehviewer.client.parser.GalleryDetailParser")]
pub fn nativeParse(mut env: JNIEnv, _: JClass, buffer: JByteBuffer, limit: jint) -> jint {
    let options = ParserOptions::default().track_ids();
    parse_marshal_inplace_with_options(&mut env, buffer, limit, options, |dom, html| {
        parse_gallery_detail(dom, html).map(|detail| (detail, parse_event_pane(dom, dom.parser())))
    })
}

#[jni_fn("com.hippo.ehviewer.client.parser.GalleryDetailParser")]
pub fn nativeParseComments(mut env: JNIEnv, _: JClass, buffer: JByteBuffer, limit: jint) -> jint {
    let options = ParserOptions::default().track_ids();
    parse_marshal_inplace_with_options(&mut env, buffer, limit, options, |dom, _| {
        parse_comments(dom)
    })
}

#[jni_fn("com.hippo.ehviewer.client.parser.GalleryDetailParser")]
pub fn nativeParsePreviews(mut env: JNIEnv, _: JClass, buffer: JByteBuffer, limit: jint) -> jint {
    let options = ParserOptions::default().track_ids();
    parse_marshal_inplace_with_options(&mut env, buffer, limit, options, |dom, _| {
        Ok((
            parse_preview_list(dom, dom.parser())?,
            parse_pages(dom, dom.parser())?,
        ))
    })
}

#[jni_fn("com.hippo.ehviewer.client.parser.VoteTagParser")]
pub fn nativeParse(mut env: JNIEnv, _: JClass, buffer: JByteBuffer, limit: jint) -> jint {
    parse_raw_marshal_inplace(&mut env, buffer, limit, parse_vote_tag)
}

#[jni_fn("com.hippo.ehviewer.client.parser.EventPaneParser")]
pub fn parseEventPane(mut env: JNIEnv, _class: JClass, input: JByteBuffer, limit: jint) -> jint {
    parse_marshal_inplace(&mut env, input, limit, |dom, _| {
        Ok(parse_event_pane(dom, dom.parser()))
    })
}

#[jni_fn("com.hippo.ehviewer.client.parser.TorrentParserKt")]
pub fn parseTorrent(mut env: JNIEnv, _class: JClass, buffer: JByteBuffer, limit: jint) -> jint {
    parse_marshal_inplace(&mut env, buffer, limit, |dom, _| {
        parse_torrent_list(dom, dom.parser())
    })
}

#[jni_fn("com.hippo.ehviewer.client.parser.UserConfigParser")]
pub fn parseFavCat(mut env: JNIEnv, _class: JClass, buffer: JByteBuffer, limit: jint) -> jint {
    parse_marshal_inplace(&mut env, buffer, limit, |_, body| Ok(parse_fav_cat(body)))
}

#[jni_fn("com.hippo.ehviewer.client.parser.ArchiveParserKt")]
pub fn parseArchives(
    mut env: JNIEnv,
    _class: JClass,
    buffer: JByteBuffer,
    limit: jint,
    parse_funds: jboolean,
) -> jint {
    if parse_funds == JNI_TRUE {
        parse_marshal_inplace(&mut env, buffer, limit, |dom, html| {
            parse_archives_with_funds(dom, dom.parser(), html)
        })
    } else {
        parse_marshal_inplace(&mut env, buffer, limit, |dom, html| {
            parse_archives(dom, dom.parser(), html)
        })
    }
}

#[jni_fn("com.hippo.ehviewer.client.parser.ArchiveParserKt")]
pub fn parseArchiveUrl(mut env: JNIEnv, _class: JClass, buffer: JByteBuffer, limit: jint) -> jint {
    parse_marshal_inplace(&mut env, buffer, limit, |dom, html| {
        parse_archive_url(dom, dom.parser(), html)
    })
}

#[jni_fn("com.hippo.ehviewer.client.parser.ProfileParser")]
pub fn parseProfileUrl(mut env: JNIEnv, _class: JClass, buffer: JByteBuffer, limit: jint) -> jint {
    parse_marshal_inplace(&mut env, buffer, limit, |dom, _| {
        parse_profile_url(dom, dom.parser())
    })
}

#[jni_fn("com.hippo.ehviewer.client.parser.ProfileParser")]
pub fn parseProfile(mut env: JNIEnv, _class: JClass, buffer: JByteBuffer, limit: jint) -> jint {
    parse_marshal_inplace(&mut env, buffer, limit, |dom, _| {
        parse_profile(dom, dom.parser())
    })
}

#[inline]
pub fn parse_marshal_inplace<F, R>(env: &mut JNIEnv, str: JByteBuffer, limit: jint, f: F) -> i32
where
    F: Fn(&mut VDom, &str) -> Result<R>,
    R: Serialize,
{
    parse_marshal_inplace_with_options(env, str, limit, ParserOptions::default(), f)
}

pub fn parse_marshal_inplace_with_options<F, R>(
    env: &mut JNIEnv,
    str: JByteBuffer,
    limit: jint,
    options: ParserOptions,
    f: F,
) -> i32
where
    F: Fn(&mut VDom, &str) -> Result<R>,
    R: Serialize,
{
    parse_raw_marshal_inplace(env, str, limit, |html| {
        let mut dom = tl::parse(html, options)?;
        ensure!(dom.version().is_some(), EhError::IpBanned(html.to_string()));
        f(&mut dom, html)
    })
}

pub fn parse_raw_marshal_inplace<F, R>(env: &mut JNIEnv, str: JByteBuffer, limit: jint, f: F) -> i32
where
    F: Fn(&str) -> Result<R>,
    R: Serialize,
{
    jni_throwing(env, |env| {
        let buffer = deref_mut_direct_bytebuffer(env, &str)?;
        let parsed = {
            let body = input_body(buffer, limit)?;
            f(body)
        };
        match parsed {
            Ok(value) => serialize_to_buffer(buffer, &value),
            Err(err) => {
                if let Some(value) = err.downcast_ref::<EhError>() {
                    serialize_to_buffer(buffer, value)
                } else {
                    Err(err)
                }
            }
        }
    })
}

#[unsafe(no_mangle)]
// The JVM discovery ABI requires this exact exported identifier.
#[allow(non_snake_case)]
pub extern "system" fn JNI_OnLoad(_: JavaVM, _: *mut c_void) -> jint {
    android_logger::init_once(Config::default().with_max_level(LevelFilter::Debug));
    JNI_VERSION_1_6
}
