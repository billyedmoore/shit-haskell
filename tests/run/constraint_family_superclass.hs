{-# LANGUAGE ConstraintKinds #-}
{-# LANGUAGE DataKinds #-}
{-# LANGUAGE FlexibleInstances #-}
{-# LANGUAGE GADTs #-}
{-# LANGUAGE MultiParamTypeClasses #-}
{-# LANGUAGE PolyKinds #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE ScopedTypeVariables #-}
{-# LANGUAGE TypeFamilies #-}
{-# LANGUAGE TypeOperators #-}
{-# LANGUAGE UndecidableInstances #-}
{-# LANGUAGE UndecidableSuperClasses #-}
module Main where

import Data.Kind (Constraint, Type)
import Data.Proxy

data SList (xs :: [k]) where
  SNil  :: SList '[]
  SCons :: SListI xs => SList (x ': xs)

class SListI (xs :: [k]) where
  sList :: SList xs

instance SListI '[] where
  sList = SNil

instance SListI xs => SListI (x ': xs) where
  sList = SCons

type family AllF (c :: k -> Constraint) (xs :: [k]) :: Constraint where
  AllF c '[]       = ()
  AllF c (x ': xs) = (c x, All c xs)

class (AllF c xs, SListI xs) => All (c :: k -> Constraint) (xs :: [k])
instance All c '[]
instance (c x, All c xs) => All c (x ': xs)

data NP (f :: k -> Type) (xs :: [k]) where
  Nil  :: NP f '[]
  (:*) :: f x -> NP f xs -> NP f (x ': xs)
infixr 5 :*

newtype K a b = K a

cpureNP :: forall c xs proxy f. All c xs => proxy c -> (forall a. c a => f a) -> NP f xs
cpureNP p f = case sList :: SList xs of
  SNil  -> Nil
  SCons -> f :* cpureNP p f

class Label a where
  label :: Proxy a -> String

instance Label Int  where label _ = "Int"
instance Label Bool where label _ = "Bool"
instance Label Char where label _ = "Char"

labelK :: forall a. Label a => K String a
labelK = K (label (Proxy :: Proxy a))

showNP :: NP (K String) xs -> [String]
showNP Nil = []
showNP (K s :* xs) = s : showNP xs

type family SameShapeAs (xs :: [k1]) (ys :: [k2]) :: Constraint where
  SameShapeAs '[]       ys = ys ~ '[]
  SameShapeAs (x ': xs) ys = (ys ~ (Head ys ': Tail ys))

type family Head (xs :: [k]) :: k where
  Head (x ': xs) = x

type family Tail (xs :: [k]) :: [k] where
  Tail (x ': xs) = xs

class (SameShapeAs xs ys, SameShapeAs ys xs) => SameLen (xs :: [k1]) (ys :: [k2])
instance SameLen '[] '[]
instance SameLen xs ys => SameLen (x ': xs) (y ': ys)

type family AllZipF (c :: k1 -> k2 -> Constraint) (xs :: [k1]) (ys :: [k2]) :: Constraint where
  AllZipF c '[]       ys = ()
  AllZipF c (x ': xs) ys = (c x (Head ys), AllZip c xs (Tail ys))

class (AllZipF c xs ys, SameLen xs ys) => AllZip (c :: k1 -> k2 -> Constraint) (xs :: [k1]) (ys :: [k2])
instance AllZip c '[] '[]
instance (c x y, AllZip c xs ys) => AllZip c (x ': xs) (y ': ys)

transNP :: AllZip c xs ys => proxy c -> (forall x y. c x y => f x -> g y) -> NP f xs -> NP g ys
transNP _ _ Nil       = Nil
transNP p t (x :* xs) = t x :* transNP p t xs

class ShowPair a b where
  showPair :: a -> Proxy b -> String

instance ShowPair Int Bool  where showPair n _ = show n ++ ":Bool"
instance ShowPair Bool Char where showPair b _ = show b ++ ":Char"

copyEmpty :: SameLen xs ys => NP f xs -> NP g ys -> Bool
copyEmpty Nil Nil = True
copyEmpty (_ :* _) (_ :* _) = False

main :: IO ()
main = do
  let np :: NP (K String) '[Int, Bool, Char]
      np = cpureNP (Proxy :: Proxy Label) labelK
  print (showNP np)
  print (copyEmpty (Nil :: NP (K Int) '[]) (Nil :: NP (K Bool) '[]))
  let src :: NP (K Int) '[Int, Bool]
      src = K 1 :* K 2 :* Nil
      dst :: NP (K String) '[Bool, Char]
      dst = transNP (Proxy :: Proxy ShowPair) (\ (K n) -> K (show (n + 10))) src
  print (showNP dst)

