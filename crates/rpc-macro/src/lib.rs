//! Procedural macros for generating RPC client and server code.

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemForeignMod, ReturnType, parse_macro_input};

/// Generate RPC client and server traits from a function signature list.
///
/// # Example
///
/// ```ignore
/// rpc! {
///     fn add(a: i32, b: i32) -> i32;
///     fn greet(name: String) -> String;
/// }
/// ```
///
/// This generates:
/// - `client::Client<T>` - Client implementation with typed methods
/// - `server::Server` - Server trait to implement
#[proc_macro]
pub fn rpc(input: TokenStream) -> TokenStream {
    let foreign_mod = parse_macro_input!(input as ItemForeignMod);

    let mut client_methods = Vec::new();
    let mut server_methods = Vec::new();
    let mut dispatch_arms = Vec::new();
    let mut schema_entries = Vec::new();

    for item in &foreign_mod.items {
        if let syn::ForeignItem::Fn(func) = item {
            let method_name = &func.sig.ident;
            let method_str = method_name.to_string();

            // Extract parameters
            let params: Vec<_> = func
                .sig
                .inputs
                .iter()
                .filter_map(|arg| {
                    if let syn::FnArg::Typed(pat_type) = arg {
                        Some(pat_type)
                    } else {
                        None
                    }
                })
                .collect();

            let param_names: Vec<_> = params.iter().map(|p| &p.pat).collect();

            let param_types: Vec<_> = params.iter().map(|p| &p.ty).collect();

            // Extract return type
            let return_type = match &func.sig.output {
                ReturnType::Default => quote! { () },
                ReturnType::Type(_, ty) => quote! { #ty },
            };

            // Generate client method
            client_methods.push(quote! {
                pub async fn #method_name(&self, #(#param_names: #param_types),*) -> rpc_core::Result<#return_type> {
                    let params = self.codec.encode(&(#(#param_names,)*))?;
                    let request = rpc_core::RpcRequest {
                        id: self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
                        method: #method_str.to_string(),
                        params,
                    };
                    let msg_data = self.codec.encode(&request)?;
                    let msg = rpc_core::Message::new(msg_data);

                    self.transport.lock().await.send(msg).await?;
                    let response_msg = self.transport.lock().await.recv().await?;
                    let response: rpc_core::RpcResponse = self.codec.decode(&response_msg.data)?;

                    match response.result {
                        rpc_core::ResponseResult::Ok(data) => {
                            let result = self.codec.decode(&data)?;
                            Ok(result)
                        }
                        rpc_core::ResponseResult::Err(e) => {
                            Err(rpc_core::Error::remote(e))
                        }
                    }
                }
            });

            // Generate server trait method
            server_methods.push(quote! {
                async fn #method_name(&self, #(#param_names: #param_types),*) -> #return_type;
            });

            // Generate dispatch arm
            dispatch_arms.push(quote! {
                #method_str => {
                    let params: (#(#param_types,)*) = codec.decode(&request.params)?;
                    let result = server.#method_name(#(params.#param_names),*).await;
                    let result_data = codec.encode(&result)?;
                    rpc_core::ResponseResult::Ok(result_data)
                }
            });

            // Generate schema entry
            schema_entries.push(quote! {
                {
                    use ::schema::Schema;
                    schema_map.insert(
                        #method_str.to_string(),
                        MethodSchema {
                            name: #method_str.to_string(),
                            params: {
                                let schema_type = <(#(#param_types,)*)>::schema();
                                ::schema_openapi::to_openapi_schema(&schema_type)
                            },
                            returns: {
                                let schema_type = <#return_type>::schema();
                                ::schema_openapi::to_openapi_schema(&schema_type)
                            },
                        }
                    );
                }
            });
        }
    }

    // Rebuild dispatch logic properly
    let mut final_dispatch_arms = Vec::new();

    for item in &foreign_mod.items {
        if let syn::ForeignItem::Fn(func) = item {
            let method_name = &func.sig.ident;
            let method_str = method_name.to_string();

            let params: Vec<_> = func
                .sig
                .inputs
                .iter()
                .filter_map(|arg| {
                    if let syn::FnArg::Typed(pat_type) = arg {
                        Some(pat_type)
                    } else {
                        None
                    }
                })
                .collect();

            let param_types: Vec<_> = params.iter().map(|p| &p.ty).collect();
            let param_count = params.len();
            let indices: Vec<_> = (0..param_count).map(syn::Index::from).collect();

            final_dispatch_arms.push(quote! {
                #method_str => {
                    let params: (#(#param_types,)*) = codec.decode(&request.params)?;
                    let result = server.#method_name(#(params.#indices),*).await;
                    let result_data = codec.encode(&result)?;
                    rpc_core::ResponseResult::Ok(result_data)
                }
            });
        }
    }

    let expanded = quote! {
        /// Schema information for a single RPC method
        #[derive(Debug, Clone)]
        pub struct MethodSchema {
            pub name: String,
            pub params: ::serde_json::Value,
            pub returns: ::serde_json::Value,
        }

        pub mod client {
            use rpc_core::{Transport, Codec, Message, RpcRequest, RpcResponse, ResponseResult};
            use std::sync::Arc;
            use tokio::sync::Mutex;
            use std::sync::atomic::{AtomicU64, Ordering};
            use std::collections::HashMap;
            use super::MethodSchema;

            pub struct Client<T, C>
            where
                T: Transport + 'static,
                C: Codec + 'static,
            {
                transport: Arc<Mutex<T>>,
                codec: Arc<C>,
                next_id: Arc<AtomicU64>,
            }

            impl<T, C> Client<T, C>
            where
                T: Transport + 'static,
                C: Codec + 'static,
            {
                pub fn new(transport: T, codec: C) -> Self {
                    Self {
                        transport: Arc::new(Mutex::new(transport)),
                        codec: Arc::new(codec),
                        next_id: Arc::new(AtomicU64::new(1)),
                    }
                }

                /// Get schema information for all RPC methods
                pub fn schema() -> HashMap<String, MethodSchema> {
                    let mut schema_map = HashMap::new();
                    #(#schema_entries)*
                    schema_map
                }

                #(#client_methods)*
            }
        }

        pub mod server {
            use rpc_core::{Transport, Codec, Message, RpcRequest, RpcResponse, ResponseResult};

            pub trait Server: Send + Sync {
                #(#server_methods)*
            }

            pub async fn serve<S, T, C>(
                server: S,
                mut transport: T,
                codec: C,
            ) -> rpc_core::Result<()>
            where
                S: Server + 'static,
                T: Transport + 'static,
                C: Codec + 'static,
            {
                loop {
                    let msg = transport.recv().await?;
                    let request: RpcRequest = codec.decode(&msg.data)?;

                    let result = match request.method.as_str() {
                        #(#final_dispatch_arms)*
                        method => {
                            rpc_core::ResponseResult::Err(
                                format!("method not found: {}", method)
                            )
                        }
                    };

                    let response = RpcResponse {
                        id: request.id,
                        result,
                    };

                    let response_data = codec.encode(&response)?;
                    let response_msg = Message::new(response_data);
                    transport.send(response_msg).await?;
                }
            }
        }
    };

    TokenStream::from(expanded)
}
