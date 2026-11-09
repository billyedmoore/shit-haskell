module Data.Coerce (coerce) where

import Base

foreign import prim "unsafeCoerce" unsafeCoerceC :: a -> b

-- Newtypes are erased at run time, so a coercion is the identity on the representation.
coerce :: a -> b
coerce x = unsafeCoerceC x
