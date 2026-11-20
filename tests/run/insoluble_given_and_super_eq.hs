{-# LANGUAGE ConstraintKinds #-}
{-# LANGUAGE DataKinds #-}
{-# LANGUAGE FlexibleContexts #-}
{-# LANGUAGE FlexibleInstances #-}
{-# LANGUAGE FunctionalDependencies #-}
{-# LANGUAGE GADTs #-}
{-# LANGUAGE KindSignatures #-}
{-# LANGUAGE MultiParamTypeClasses #-}
{-# LANGUAGE PolyKinds #-}
{-# LANGUAGE ScopedTypeVariables #-}
{-# LANGUAGE TypeFamilies #-}
{-# LANGUAGE TypeOperators #-}
{-# LANGUAGE UndecidableInstances #-}
{-# LANGUAGE UndecidableSuperClasses #-}
module Main where

import Data.Kind (Type)
import GHC.TypeLits (ErrorMessage(..), TypeError)

type family Append (xs :: [Type]) (ys :: [Type]) :: [Type] where
  Append '[]       ys = ys
  Append (x ': xs) ys = x ': Append xs ys

class (is ~ '[i]) => HasSingleIndex (is :: [Type]) (i :: Type) | is -> i
instance HasSingleIndex '[i] i

class (is ~ '[]) => AcceptsEmpty (is :: [Type])
instance AcceptsEmpty '[]
instance (TypeError ('Text "Non-empty index list"), (i ': is) ~ '[]) => AcceptsEmpty (i ': is)

data IxList (is :: [Type]) where
  IxNil  :: IxList '[]
  IxCons :: i -> IxList is -> IxList (i ': is)

unconsSingle :: HasSingleIndex is i => IxList is -> i
unconsSingle (IxCons x IxNil) = x

appendEmpty :: forall is js. AcceptsEmpty is => IxList js -> IxList (Append is js)
appendEmpty xs = xs

(#) :: Int -> Int -> Int
a # b = a * 10 + b

main :: IO ()
main = do
  print (unconsSingle (IxCons (42 :: Int) IxNil))
  let ys :: IxList '[Int]
      ys = appendEmpty (IxCons (7 :: Int) IxNil)
  print (unconsSingle ys)
  print ((#) 3 4)
  print (3 # 5)
