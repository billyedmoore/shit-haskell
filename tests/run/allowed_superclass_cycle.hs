{-# LANGUAGE UndecidableSuperClasses #-}
module CycleAllowed where

class B a => A a
class A a => B a
