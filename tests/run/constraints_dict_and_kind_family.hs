{-# LANGUAGE AllowAmbiguousTypes #-}
{-# LANGUAGE ConstraintKinds #-}
{-# LANGUAGE DataKinds #-}
{-# LANGUAGE FlexibleContexts #-}
{-# LANGUAGE FlexibleInstances #-}
{-# LANGUAGE FunctionalDependencies #-}
{-# LANGUAGE GADTs #-}
{-# LANGUAGE MultiParamTypeClasses #-}
{-# LANGUAGE PolyKinds #-}
{-# LANGUAGE QuantifiedConstraints #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE ScopedTypeVariables #-}
{-# LANGUAGE TypeApplications #-}
{-# LANGUAGE TypeFamilies #-}
{-# LANGUAGE TypeOperators #-}
{-# LANGUAGE UndecidableInstances #-}
{-# LANGUAGE UndecidableSuperClasses #-}
module Main where

import Data.Kind (Constraint, Type)
import Data.Proxy (Proxy(..))
import Type.Reflection

data Dict (c :: Constraint) where
  Dict :: c => Dict c

newtype a :- b = Sub (a => Dict b)

(\\) :: a => (b => r) -> (a :- b) -> r
r \\ Sub Dict = r
infixl 1 \\

trans :: (b :- c) -> (a :- b) -> (a :- c)
trans f g = Sub $ Dict \\ f \\ g

refl :: a :- a
refl = Sub Dict

forceSub :: a => (a :- b) -> ()
forceSub (Sub Dict) = ()

-- Quantified constraint in a GADT constructor
data Dict1 (p :: k -> Constraint) where
  Dict1 :: (forall a. p a) => Dict1 p

useDict1 :: forall p a. Dict1 p -> Dict (p a)
useDict1 Dict1 = Dict

class AlwaysTrue (a :: Type) where
  alwaysVal :: Proxy a -> Int

instance AlwaysTrue a where
  alwaysVal _ = 99

-- Empty constraint predicate ()
class () => Trivial a where
  triv :: a -> Int
instance Trivial Int where
  triv = id

-- withTypeable (typeRepKind tr) matching Typeable k
class HasDict c e | e -> c where
  evidence :: e -> Dict c

instance HasDict (Typeable k, Typeable a) (TypeRep (a :: k)) where
  evidence tr = withTypeable tr $ withTypeable (typeRepKind tr) Dict

-- Kind-dispatched instances and associated type family (ForallV' / InstV)
class (forall a. p a) => Forall (p :: k -> Constraint)
instance (forall a. p a) => Forall (p :: k -> Constraint)

inst :: Forall p :- p a
inst = Sub Dict

class InstV (p :: k) (c :: Constraint) where
  type ForallV' p :: Constraint
  instV :: Proxy p -> ForallV' p :- c

instance p ~ c => InstV (p :: Constraint) c where
  type ForallV' (p :: Constraint) = p
  instV _ = Sub Dict

instance p a ~ c => InstV (p :: k -> Constraint) c where
  type ForallV' (p :: k -> Constraint) = Forall p
  instV _ = inst

main :: IO ()
main = do
  let s :: Eq Int :- Eq Int
      s = trans refl refl
  print (forceSub s)
  case useDict1 (Dict1 :: Dict1 AlwaysTrue) :: Dict (AlwaysTrue Bool) of
    Dict -> print (alwaysVal (Proxy :: Proxy Bool))
  print (triv (5 :: Int))
  case evidence (typeRep @Int) of
    Dict -> print (typeRep @Int)
  case (instV (Proxy :: Proxy (Eq Int)) :: ForallV' (Eq Int) :- Eq Int) of
    Sub Dict -> print ((1 :: Int) == 1)
  case (instV (Proxy :: Proxy AlwaysTrue) :: ForallV' AlwaysTrue :- AlwaysTrue Char) of
    Sub Dict -> print (alwaysVal (Proxy :: Proxy Char))
