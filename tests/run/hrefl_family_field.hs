{-# LANGUAGE GADTs, DataKinds, KindSignatures, TypeFamilies, TypeOperators, RankNTypes, ScopedTypeVariables, FlexibleInstances, TypeApplications #-}
-- HRefl refines through a field whose type is a family that reduces (Cabal's RuleCommands Eq
-- instance), and defaulting a type that Typeable also constrains (`mk 3`).
import Data.Kind (Type)
import Data.Type.Bool (If)
import Data.Type.Equality (type (==), (:~~:) (HRefl))
import Type.Reflection (TypeRep, SomeTypeRep, eqTypeRep, typeRep, Typeable)
data Scope = User | System
data Cmd (s :: Scope) where
  Cmd :: forall arg s. Eq arg => { val :: arg, rep :: If (s == 'System) SomeTypeRep (TypeRep arg) } -> Cmd s
instance Eq (Cmd 'User) where
  Cmd{val = v1 :: arg1, rep = tr1} == Cmd{val = v2 :: arg2, rep = tr2}
    | Just HRefl <- eqTypeRep tr1 tr2 = v1 == v2
  _ == _ = False
mk :: forall a. (Typeable a, Eq a) => a -> Cmd 'User
mk x = Cmd x (typeRep @a)
main = print (mk (3 :: Int) == mk 3, mk 'a' == mk (1 :: Int), mk True == mk False)
