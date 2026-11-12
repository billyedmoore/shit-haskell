{-# LANGUAGE DefaultSignatures, ScopedTypeVariables #-}
module Str (Structured (structure), nominal) where
import Data.Typeable
class Typeable a => Structured a where
  structure :: Proxy a -> String
  default structure :: Show a => Proxy a -> String
  structure _ = "default"
  hidden' :: Tagged a Int
  hidden' = Tagged (length (structure (Proxy :: Proxy a)))
newtype Tagged a b = Tagged {untag :: b}
nominal :: Typeable a => Proxy a -> String
nominal p = show (typeRep p)
