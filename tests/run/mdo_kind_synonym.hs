{-# LANGUAGE RecursiveDo, StandaloneKindSignatures, PolyKinds, RankNTypes #-}
-- `mdo` in a module that does not import Control.Monad.Fix; a type synonym whose right-hand
-- side has a kind signature (as th-compat writes `type Splice = Code :: ...`).
module Main where

import Data.IORef
import Data.Kind (Type)

data Node = Node { label :: Int, next :: Node }

type Wrap :: (Type -> Type) -> Type -> Type
newtype Wrap f a = Wrap (f a)

type Alias = Wrap :: (Type -> Type) -> Type -> Type

unwrap :: Alias Maybe Int -> Maybe Int
unwrap (Wrap m) = m

main :: IO ()
main = do
  -- A cycle of two nodes built in IO.
  a <- mdo
    r <- newIORef (0 :: Int)
    let x = Node 1 y
        y = Node 2 x
    writeIORef r (label (next x))
    v <- readIORef r
    return (Node v (next y))
  print (label a, label (next a), label (next (next a)))
  print (unwrap (Wrap (Just 5)))
