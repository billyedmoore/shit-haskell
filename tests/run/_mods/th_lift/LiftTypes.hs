{-# LANGUAGE DeriveLift #-}
module LiftTypes (Tree (..), Pair (..), (:|) (..)) where

import Language.Haskell.TH.Syntax (Lift)

data Tree = Leaf Int | Node Tree String Tree deriving (Show, Lift)

data Pair = Pair Char [Double] deriving (Show, Lift)

infixr 5 :|
data (:|) a b = a :| b deriving (Show, Lift)
