module BootBinderOrder where

value :: forall b a. a -> b -> a
value x _ = x
