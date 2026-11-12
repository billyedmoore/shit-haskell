{-# LANGUAGE GADTs, DataKinds, KindSignatures, TypeFamilies, TypeOperators, ConstraintKinds, RankNTypes #-}
-- A constructor's constraint-family context (`If (s == System) (a ~ String) ()`) reduces to an
-- equality once a later GADT match refines `s` (Cabal's ruleExecCmd).
module Main where

import Data.Kind (Constraint, Type)
import Data.Type.Bool (If)
import Data.Type.Equality (type (==))

data Scope = User | System

data SScope (s :: Scope) where
  SUser :: SScope 'User
  SSystem :: SScope 'System

data Cmd (s :: Scope) where
  Cmd :: forall a s. If (s == 'System) (a ~ String) (() :: Constraint) => (a -> Int) -> a -> Cmd s

runCmd :: SScope s -> Cmd s -> String -> Int
runCmd scope (Cmd f a) input = case scope of
  SUser -> f a
  SSystem -> f input + f a

-- Built at 'User, the context reduces to the empty constraint.
userCmd :: Cmd 'User
userCmd = Cmd (* 2) (21 :: Int)

main :: IO ()
main = do
  print (runCmd SUser userCmd "")
  print (runCmd SUser (Cmd (+ 1) (41 :: Int)) "ignored")
  print (runCmd SSystem (Cmd length "abc") "hello")
