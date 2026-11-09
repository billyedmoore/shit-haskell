module Unsafe.Coerce (unsafeCoerce) where

foreign import prim "unsafeCoerce" unsafeCoerce :: a -> b
