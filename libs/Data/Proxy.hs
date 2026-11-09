module Data.Proxy (Proxy(..), asProxyTypeOf) where

import Base

data Proxy t = Proxy

instance Show (Proxy t) where
  showsPrec _ _ = showString "Proxy"

instance Eq (Proxy t) where
  _ == _ = True

asProxyTypeOf :: a -> proxy a -> a
asProxyTypeOf x _ = x
