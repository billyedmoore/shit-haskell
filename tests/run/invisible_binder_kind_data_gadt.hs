{-# LANGUAGE PolyKinds,TypeFamilies,TypeAbstractions,NoTypeApplications,CUSKs,GADTs #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)
data T @(k :: Type) (a :: k) :: Type where
  T :: T a
main :: IO ()
main = pure ()
