{-# LANGUAGE FlexibleContexts #-}
{-# LANGUAGE FlexibleInstances #-}
{-# LANGUAGE FunctionalDependencies #-}
{-# LANGUAGE MultiParamTypeClasses #-}
{-# LANGUAGE PolyKinds #-}
{-# LANGUAGE RankNTypes #-}
{-# LANGUAGE UndecidableInstances #-}
{-# LANGUAGE DataKinds #-}
module Main where

import Data.Proxy
import GHC.TypeLits

class Reifies s a | s -> a where
  reflect :: p s -> a

instance KnownNat n => Reifies n Integer where
  reflect = natVal

instance KnownSymbol s => Reifies s String where
  reflect = symbolVal

data Tag a = Tag
data TagBox a = TagBox

instance Reifies (TagBox a) a => Reifies (Tag a) a where
  reflect _ = reflect (Proxy :: Proxy (TagBox a))

instance Reifies (TagBox Int) Int where
  reflect _ = 42

withTag :: Reifies (TagBox a) a => (Reifies (Tag a) a => Proxy (Tag a) -> r) -> Proxy (Tag a) -> r
withTag k p = k p

main :: IO ()
main = do
  print (reflect (Proxy :: Proxy 7))
  putStrLn (reflect (Proxy :: Proxy "hello"))
  print (withTag reflect (Proxy :: Proxy (Tag Int)))
