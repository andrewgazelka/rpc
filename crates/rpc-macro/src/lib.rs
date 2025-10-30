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
/// - `server::Service` - Low-level message-passing service trait
/// - `server::AsyncService` - High-level async method service trait
/// - Blanket impl to convert AsyncService -> Service
#[proc_macro]
pub fn rpc(input: TokenStream) -> TokenStream {
    let foreign_mod = parse_macro_input!(input as ItemForeignMod);

    let mut client_methods = Vec::new();
    let mut dyn_client_trait_methods = Vec::new();
    let mut dyn_client_impl_methods = Vec::new();
    let mut async_service_methods = Vec::new();
    let mut msg_variants = Vec::new();
    let mut intake_match_arms = Vec::new();
    let mut dispatch_arms = Vec::new();
    let mut schema_entries = Vec::new();
    let mut wit_methods = Vec::new();

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

            // Generate trait method (returns Pin<Box<dyn Future>>)
            dyn_client_trait_methods.push(quote! {
                fn #method_name(
                    &self,
                    #(#param_names: #param_types),*
                ) -> std::pin::Pin<Box<dyn std::future::Future<Output = rpc_core::Result<#return_type>> + Send + '_>>;
            });

            // Generate trait impl method (calls the async method and boxes the future)
            dyn_client_impl_methods.push(quote! {
                fn #method_name(
                    &self,
                    #(#param_names: #param_types),*
                ) -> std::pin::Pin<Box<dyn std::future::Future<Output = rpc_core::Result<#return_type>> + Send + '_>> {
                    Box::pin(self.#method_name(#(#param_names),*))
                }
            });

            // Generate client method
            client_methods.push(quote! {
                pub async fn #method_name(&self, #(#param_names: #param_types),*) -> rpc_core::Result<#return_type> {
                    let span = rpc_core::tracing::debug_span!(
                        "rpc_client_call",
                        method = #method_str,
                        request_id = rpc_core::tracing::field::Empty
                    );

                    let params = self.codec.encode(&(#(#param_names,)*))?;
                    let request_id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                    span.record("request_id", request_id);

                    let request = rpc_core::RpcRequest {
                        id: request_id,
                        method: #method_str.to_string(),
                        params,
                    };

                    rpc_core::tracing::debug!(parent: &span, "sending request");

                    let msg_data = self.codec.encode(&request)?;
                    let msg = rpc_core::Message::new(msg_data);

                    self.transport.lock().await.send(msg).await.map_err(rpc_core::Error::transport)?;

                    rpc_core::tracing::debug!(parent: &span, "awaiting response");

                    let response_msg = self.transport.lock().await.recv().await.map_err(rpc_core::Error::transport)?;
                    let response: rpc_core::RpcResponse = self.codec.decode(&response_msg.data)?;

                    match response.result {
                        rpc_core::ResponseResult::Ok(data) => {
                            let result = self.codec.decode(&data)?;
                            rpc_core::tracing::debug!(parent: &span, ?result, "call succeeded");
                            Ok(result)
                        }
                        rpc_core::ResponseResult::Err(e) => {
                            rpc_core::tracing::warn!(parent: &span, error = %e, "call failed");
                            Err(rpc_core::Error::remote(e))
                        }
                        rpc_core::ResponseResult::StreamChunk(_) => {
                            rpc_core::tracing::error!(parent: &span, "unexpected stream chunk in non-streaming call");
                            Err(rpc_core::Error::other("unexpected stream chunk in non-streaming call"))
                        }
                        rpc_core::ResponseResult::StreamEnd => {
                            rpc_core::tracing::error!(parent: &span, "unexpected stream end in non-streaming call");
                            Err(rpc_core::Error::other("unexpected stream end in non-streaming call"))
                        }
                    }
                }
            });

            // Generate Msg enum variant
            // Use tuple for params: single param = (T,), multiple = (T1, T2, ...), zero = ()
            let msg_param_type = quote! { (#(#param_types,)*) };

            msg_variants.push(quote! {
                #method_name(#msg_param_type, tokio::sync::oneshot::Sender<#return_type>)
            });

            // Generate intake match arm for blanket impl
            // Unpack tuple into individual parameters
            let indices: Vec<_> = (0..params.len()).map(syn::Index::from).collect();
            let field_accesses: Vec<_> = indices
                .iter()
                .map(|idx| {
                    quote! { params.#idx }
                })
                .collect();

            intake_match_arms.push(quote! {
                Msg::#method_name(params, tx) => {
                    let result = self.#method_name(#(#field_accesses),*).await;
                    let _ = tx.send(result);
                }
            });

            // Generate AsyncService trait method
            async_service_methods.push(quote! {
                async fn #method_name(&self, #(#param_names: #param_types),*) -> #return_type;
            });

            // Generate dispatch arm (now uses Service::intake)
            dispatch_arms.push(quote! {
                #method_str => {
                    let params: (#(#param_types,)*) = codec.decode(&request.params)?;
                    let result = server.#method_name(#(params.#param_names),*).await;
                    let result_data = codec.encode(&result)?;
                    rpc_core::ResponseResult::Ok(result_data)
                }
            });

            // Generate schema entry (only when openapi feature is enabled)
            schema_entries.push(quote! {
                #[cfg(feature = "openapi")]
                {
                    use ::schema::Schema;
                    schema_map.insert(
                        #method_str.to_string(),
                        MethodSchema {
                            name: #method_str.to_string(),
                            params: {
                                let schema_type = <(#(#param_types,)*)>::schema();
                                ::schema_openapi::schema_type_to_openapi(&schema_type)
                            },
                            returns: {
                                let schema_type = <#return_type>::schema();
                                ::schema_openapi::schema_type_to_openapi(&schema_type)
                            },
                        }
                    );
                }
            });

            // Generate WIT method signature (only when wit feature is enabled)
            // Convert param names to strings for WIT generation
            let _param_name_strs: Vec<_> = param_names
                .iter()
                .map(|p| {
                    // Extract identifier from pattern
                    quote! { stringify!(#p) }.to_string()
                })
                .collect();

            wit_methods.push(quote! {
                #[cfg(feature = "wit")]
                {
                    use ::schema::Schema;
                    use ::schema_wit::schema_type_to_wit;

                    wit_output.push_str(&format!("    {}: func(", #method_str));

                    // Generate parameter list with names
                    let param_parts: Vec<String> = vec![
                        #(
                            {
                                let param_schema = <#param_types>::schema();
                                let param_wit = schema_type_to_wit(&param_schema, None);
                                format!("{}: {}", stringify!(#param_names), param_wit)
                            }
                        ),*
                    ];

                    wit_output.push_str(&param_parts.join(", "));
                    wit_output.push(')');

                    // Only add return type if it's not unit ()
                    let return_schema = <#return_type>::schema();
                    // Check if it's an empty object (unit type)
                    let is_unit = matches!(&return_schema.kind, ::schema::TypeKind::Object { properties, required } if properties.is_empty() && required.is_empty());

                    if !is_unit {
                        let return_wit = schema_type_to_wit(&return_schema, None);
                        wit_output.push_str(" -> ");
                        wit_output.push_str(&return_wit);
                    }

                    wit_output.push('\n');
                }
            });
        }
    }

    // Rebuild dispatch logic using Service::intake
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
            let _param_names: Vec<_> = params.iter().map(|p| &p.pat).collect();
            let param_count = params.len();
            let _indices: Vec<_> = (0..param_count).map(syn::Index::from).collect();

            let _return_type = match &func.sig.output {
                ReturnType::Default => quote! { () },
                ReturnType::Type(_, ty) => quote! { #ty },
            };

            // Decode params as tuple and pass to Msg enum
            final_dispatch_arms.push(quote! {
                #method_str => {
                    let params: (#(#param_types,)*) = codec.decode(&request.params)?;
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    server.intake(Msg::#method_name(params, tx)).await;
                    let result = rx.await.map_err(|_| rpc_core::Error::other("Service dropped response channel"))?;
                    let result_data = codec.encode(&result)?;
                    rpc_core::ResponseResult::Ok(result_data)
                }
            });
        }
    }

    let expanded = quote! {
        /// Schema information for a single RPC method
        #[cfg(feature = "openapi")]
        #[derive(Debug, Clone)]
        pub struct MethodSchema {
            pub name: String,
            pub params: ::serde_json::Value,
            pub returns: ::serde_json::Value,
        }

        pub mod client {
            use super::*;
            use rpc_core::{Transport, Codec, Message, RpcRequest, RpcResponse, ResponseResult};
            use std::sync::Arc;
            use tokio::sync::Mutex;
            use std::sync::atomic::{AtomicU64, Ordering};
            use std::collections::HashMap;

            /// Dynamic client trait for type erasure
            pub trait DynClient: Send + Sync {
                #(#dyn_client_trait_methods)*
            }

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
                #[cfg(feature = "openapi")]
                pub fn schema() -> HashMap<String, MethodSchema> {
                    let mut schema_map = HashMap::new();
                    #(#schema_entries)*
                    schema_map
                }

                /// Generate WIT (WebAssembly Interface Type) definition for this RPC interface
                #[cfg(feature = "wit")]
                pub fn wit_schema(interface_name: &str) -> String {
                    let mut wit_output = String::new();
                    wit_output.push_str(&format!("interface {} {{\n", interface_name));
                    #(#wit_methods)*
                    wit_output.push_str("}\n");
                    wit_output
                }

                #(#client_methods)*
            }

            impl<T, C> DynClient for Client<T, C>
            where
                T: Transport + 'static,
                C: Codec + 'static,
            {
                #(#dyn_client_impl_methods)*
            }

            /// Dynamic client that accepts method names as strings at runtime
            pub struct DynamicClient<T, C>
            where
                T: Transport + 'static,
                C: Codec + 'static,
            {
                transport: Arc<Mutex<T>>,
                codec: Arc<C>,
                next_id: Arc<AtomicU64>,
            }

            impl<T, C> DynamicClient<T, C>
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

                /// Make a single RPC call with dynamic method name
                pub async fn call(&self, method: &str, params: Vec<u8>) -> rpc_core::Result<Vec<u8>> {
                    let request_id = self.next_id.fetch_add(1, Ordering::SeqCst);
                    let request = RpcRequest {
                        id: request_id,
                        method: method.to_string(),
                        params,
                    };

                    let request_data = self.codec.encode(&request)?;
                    let request_msg = Message::new(request_data);

                    let mut transport = self.transport.lock().await;
                    transport.send(request_msg).await.map_err(rpc_core::Error::transport)?;

                    let response_msg = transport.recv().await.map_err(rpc_core::Error::transport)?;
                    drop(transport);

                    let response: RpcResponse = self.codec.decode(&response_msg.data)?;

                    if response.id != request_id {
                        return Err(rpc_core::Error::other("response ID mismatch"));
                    }

                    match response.result {
                        ResponseResult::Ok(data) => Ok(data),
                        ResponseResult::Err(e) => Err(rpc_core::Error::remote(e)),
                        ResponseResult::StreamChunk(_) => {
                            Err(rpc_core::Error::other("unexpected stream chunk in non-streaming call"))
                        }
                        ResponseResult::StreamEnd => {
                            Err(rpc_core::Error::other("unexpected stream end in non-streaming call"))
                        }
                    }
                }

            }
        }

        pub mod server {
            use super::*;
            use rpc_core::{Transport, Codec, Message, RpcRequest, RpcResponse, ResponseResult};

            // Message enum
            #[derive(Debug)]
            #[allow(non_camel_case_types)]
            pub enum Msg {
                #(#msg_variants),*
            }

            // Low-level Service trait (message-passing)
            pub trait Service: Send + Sync {
                async fn intake(&self, msg: Msg);
            }

            // High-level AsyncService trait (ergonomic async methods)
            pub trait AsyncService: Send + Sync {
                #(#async_service_methods)*
            }

            // Blanket impl: any AsyncService can be used as a Service
            impl<T: AsyncService> Service for T {
                async fn intake(&self, msg: Msg) {
                    match msg {
                        #(#intake_match_arms)*
                    }
                }
            }

            pub async fn serve<S, T, C>(
                server: S,
                mut transport: T,
                codec: C,
            ) -> rpc_core::Result<()>
            where
                S: Service + 'static,
                T: Transport + 'static,
                C: Codec + 'static,
            {
                loop {
                    let msg = transport.recv().await.map_err(rpc_core::Error::transport)?;
                    let request: RpcRequest = codec.decode(&msg.data)?;

                    let span = rpc_core::tracing::debug_span!(
                        "rpc_server_dispatch",
                        method = %request.method,
                        request_id = request.id
                    );

                    rpc_core::tracing::debug!(parent: &span, "received request");

                    let result = match request.method.as_str() {
                        #(#final_dispatch_arms)*
                        method => {
                            rpc_core::tracing::warn!(parent: &span, method = %method, "method not found");
                            rpc_core::ResponseResult::Err(
                                format!("method not found: {}", method)
                            )
                        }
                    };

                    match &result {
                        rpc_core::ResponseResult::Ok(_) => rpc_core::tracing::debug!(parent: &span, "method call succeeded"),
                        rpc_core::ResponseResult::Err(e) => rpc_core::tracing::warn!(parent: &span, error = %e, "method call failed"),
                        rpc_core::ResponseResult::StreamChunk(_) => rpc_core::tracing::debug!(parent: &span, "sent stream chunk"),
                        rpc_core::ResponseResult::StreamEnd => rpc_core::tracing::debug!(parent: &span, "stream ended"),
                    }

                    let response = RpcResponse {
                        id: request.id,
                        result,
                    };

                    let response_data = codec.encode(&response)?;
                    let response_msg = Message::new(response_data);
                    transport.send(response_msg).await.map_err(rpc_core::Error::transport)?;

                    rpc_core::tracing::debug!(parent: &span, "sent response");
                }
            }
        }
    };

    TokenStream::from(expanded)
}
