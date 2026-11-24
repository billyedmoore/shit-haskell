{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,CUSKs,GADTs #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
data T @(k :: Type) :: k2 -> Type where
  T :: T a
main :: IO ()
main = pure ()
