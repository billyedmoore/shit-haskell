{-# LANGUAGE TemplateHaskell #-}
-- | What QuickCheck's `quickCheckAll` does: the splicing module's source is read (`location`,
-- `runIO`), each `prop_` name in it is reified (`recover` skips those not defined), and a
-- polymorphic one is used at `Integer`.
module Props (allProps, here) where

import Data.Char (isAlphaNum, isSpace)
import Data.List (isPrefixOf, nub)
import Language.Haskell.TH

-- | The file (its last component), module and line of the splice.
here :: Q Exp
here = do
  Loc { loc_filename = file, loc_module = m, loc_start = (l, _) } <- location
  let base = reverse (takeWhile (/= '/') (reverse file))
  [| (base, m, l) |]

-- | Each property defined in the splicing module: its name, whether it is polymorphic, and its
-- value at a sample argument.
allProps :: Q Exp
allProps = do
  Loc { loc_filename = file } <- location
  ls <- runIO (fmap lines (readFile file))
  let idents = nub (filter ("prop_" `isPrefixOf`) (map (takeWhile (\c -> isAlphaNum c || c == '_') . dropWhile isSpace) ls))
  es <- fmap concat (mapM one idents)
  pure (ListE es)
  where
    one x = do
      found <- recover (pure Nothing) (fmap Just (reify (mkName x)))
      case found of
        Just (VarI _ ty _) -> do
          let (poly, body) = case ty of
                ForallT _ _ t -> (True, t)
                t -> (False, t)
              integer = ConT ''Integer
              mono t = case t of
                VarT _ -> integer
                AppT a b -> AppT (mono a) (mono b)
                _ -> t
              f = if poly then SigE (VarE (mkName x)) (mono body) else VarE (mkName x)
              value = case body of
                AppT (AppT ArrowT (AppT ListT _)) _ -> [| $(pure f) [1, 2, 3] |]
                AppT (AppT ArrowT _) _ -> [| $(pure f) 5 |]
                _ -> pure f
          e <- [| (x, poly, $value) |]
          pure [e]
        _ -> pure []
