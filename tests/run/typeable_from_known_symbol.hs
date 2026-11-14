{-# LANGUAGE DataKinds, KindSignatures, StandaloneDeriving, DeriveDataTypeable, ScopedTypeVariables #-}
-- Typeable of a Symbol- or Nat-kinded variable from its KnownSymbol / KnownNat given (GHC's
-- `deriving instance KnownSymbol tok => Data (EpToken tok)`).
module Main where

import Data.Data
import GHC.TypeLits

data Token (tok :: Symbol) = NoTok | Tok Int

deriving instance KnownSymbol tok => Data (Token tok)

data Sized (n :: Nat) = Sized [Int]

repOf :: forall n. KnownNat n => Sized n -> TypeRep
repOf = typeOf

symbolRep :: forall s. KnownSymbol s => Proxy s -> TypeRep
symbolRep _ = typeRep (Proxy :: Proxy s)

main :: IO ()
main = do
  let t = Tok 3 :: Token "let"
  print (typeOf t)
  print (toConstr t, toConstr (NoTok :: Token "in"))
  print (dataTypeName (dataTypeOf t))
  print (repOf (Sized [] :: Sized 4))
  print (symbolRep (Proxy :: Proxy "hello"))
