{-# LANGUAGE MultiParamTypeClasses, FunctionalDependencies, TypeFamilies, FlexibleContexts, ScopedTypeVariables #-}
-- Functional-dependency improvement against givens whose determining arguments
-- only match after type-family normalization.

type family Wrap a
type instance Wrap Int = Bool

class Access k m v | k m -> v where
  access :: Proxy k -> m -> v

data Proxy a = Proxy

instance Access Bool String Int where
  access _ s = length s

useAccess :: forall m v. Access (Wrap Int) m v => m -> v
useAccess = access (Proxy :: Proxy Bool)

useAccessRev :: forall m v. Access Bool m v => m -> v
useAccessRev = access (Proxy :: Proxy (Wrap Int))

main :: IO ()
main = do
  print (useAccess "hello" :: Int)
  print (useAccessRev "world!" :: Int)
