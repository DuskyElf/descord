use super::*;

#[derive(Debug, Clone)]
pub struct Command {
    pub name: String,
    pub custom_prefix: bool,
    pub fn_sig: Vec<ParamType>,
    pub handler_fn: HandlerFn,
    pub optional_params: Vec<bool>,
    pub permissions: Vec<String>,
    pub description: String,
}

impl Command {
    pub async fn call(&self, data: Message) -> HandlerResult {
        let split = parse_args(&data.content);
        let mut args: Vec<Value> = Vec::with_capacity(self.fn_sig.len());

        let mut idx = 1;
        while idx - 1 < self.fn_sig.len() {
            let ty = &self.fn_sig[idx - 1];
            let optional = self.optional_params[idx - 1];
            if idx < split.len() {
                match ty {
                    ParamType::String => args.push(if optional {
                        Value::StringOption(Some(split[idx].to_owned()))
                    } else {
                        Value::String(split[idx].to_owned())
                    }),
                    ParamType::Int => {
                        match split[idx].parse::<isize>() {
                            Ok(val) => args.push(if optional {
                                Value::IntOption(Some(val))
                            } else {
                                Value::Int(val)
                            }),
                            Err(_) => return Err(Box::new(DescordError::InvalidArgument {
                                param: format!("argument {}", idx),
                                expected: "integer".to_string(),
                                got: split[idx].to_owned(),
                            })),
                        }
                    },
                    ParamType::Bool => {
                        match split[idx].parse::<bool>() {
                            Ok(val) => args.push(if optional {
                                Value::BoolOption(Some(val))
                            } else {
                                Value::Bool(val)
                            }),
                            Err(_) => return Err(Box::new(DescordError::InvalidArgument {
                                param: format!("argument {}", idx),
                                expected: "bool".to_string(),
                                got: split[idx].to_owned(),
                            })),
                        }
                    },
                    ParamType::Channel => {
                        let channel_id_str = &split[idx];
                        let channel_id =
                            if channel_id_str.starts_with("<#") && channel_id_str.ends_with(">") {
                                &channel_id_str[2..channel_id_str.len() - 1]
                            } else {
                                channel_id_str
                            };
                        match fetch_channel(channel_id).await {
                            Ok(channel) => args.push(if optional {
                                Value::ChannelOption(Some(channel))
                            } else {
                                Value::Channel(channel)
                            }),
                            Err(e) => return Err(Box::new(e)),
                        }
                    }
                    ParamType::User => {
                        let user_id_str = &split[idx];
                        let user_id = if user_id_str.starts_with("<@") && user_id_str.ends_with(">")
                        {
                            &user_id_str[2..user_id_str.len() - 1]
                        } else {
                            user_id_str
                        };
                        match fetch_user(user_id).await {
                            Ok(user) => args.push(if optional {
                                Value::UserOption(Some(user))
                            } else {
                                Value::User(user)
                            }),
                            Err(e) => return Err(Box::new(e)),
                        }
                    }
                    ParamType::Args => {
                        args.push(Value::Args(
                            split[idx..].iter().map(|i| i.to_string()).collect(),
                        ));
                    }
                    _ => {}
                }
            } else if optional {
                match ty {
                    ParamType::String => args.push(Value::StringOption(None)),
                    ParamType::Int => args.push(Value::IntOption(None)),
                    ParamType::Bool => args.push(Value::BoolOption(None)),
                    ParamType::Channel => args.push(Value::ChannelOption(None)),
                    ParamType::User => args.push(Value::UserOption(None)),
                    _ => {}
                }
            } else {
                return Err(Box::new(DescordError::MissingRequiredArgument {
                    command: self.name.clone(),
                    param: format!("argument {}", idx),
                }));
            }

            idx += 1;
        }

        let fut = ((self.handler_fn)(data, args));
        let boxed_fut: std::pin::Pin<
            Box<dyn std::future::Future<Output = HandlerResult> + Send + 'static>,
        > = Box::pin(fut);

        boxed_fut.await?;

        Ok(())
    }
}
