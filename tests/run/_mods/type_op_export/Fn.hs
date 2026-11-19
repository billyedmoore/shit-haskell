{-# LANGUAGE TypeOperators, ExplicitNamespaces #-}
module Fn (type (-.->)(..), apFn) where

newtype (f -.-> g) a = Fn { unFn :: f a -> g a }

apFn :: (f -.-> g) a -> f a -> g a
apFn = unFn
