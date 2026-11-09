{-# LANGUAGE NoImplicitPrelude #-}
-- The defining module of the Prelude's names. Written against the primitive operations
-- implemented by the runtime; `Prelude` re-exports the standard names from here.
module Base (
    module Base, module GHC.Prim,
    Bool(..), Char(..), Int(..), Word(..), Float(..), Double(..), Ordering(..), IO(..), isTrue#
  ) where

import GHC.Prim
import GHC.Types

infixr 9  .
infixr 8  ^
infixl 7  *, /, `quot`, `rem`, `div`, `mod`
infixl 6  +, -
infixr 5  ++
infix  4  ==, /=, <, <=, >=, >
infixl 4  <$>, <$
infixl 1  >>=, >>
infixr 1  =<<
infixr 3  &&
infixr 2  ||
infixl 9  !!
infixr 0  $, $!
infixr 6  <>
infixl 7  %
infixl 4  <*>, *>, <*

-- ----- primitive types ------------------------------------------------------

data Integer

-- The state token of IO, as in GHC.
type World = State# RealWorld

-- A mutable reference cell (the runtime value behind IORef).
data Ref a

type String = [Char]
type FilePath = String
type ShowS = String -> String
type ReadS a = String -> [(a, String)]
type Rational = Ratio Integer

data Maybe a = Nothing | Just a
data Either a b = Left a | Right b
data Ratio a = !a :% !a


-- ----- primitives -------------------------------------------------------------

foreign import prim "intAdd" primIntAdd :: Int -> Int -> Int
foreign import prim "intSub" primIntSub :: Int -> Int -> Int
foreign import prim "intMul" primIntMul :: Int -> Int -> Int
foreign import prim "intNegate" primIntNegate :: Int -> Int
foreign import prim "intQuot" primIntQuot :: Int -> Int -> Int
foreign import prim "intRem" primIntRem :: Int -> Int -> Int
foreign import prim "intDiv" primIntDiv :: Int -> Int -> Int
foreign import prim "intMod" primIntMod :: Int -> Int -> Int
foreign import prim "intEq" primIntEq :: Int -> Int -> Bool
foreign import prim "intLt" primIntLt :: Int -> Int -> Bool
foreign import prim "intLe" primIntLe :: Int -> Int -> Bool
foreign import prim "intToInteger" primIntToInteger :: Int -> Integer
foreign import prim "intToDouble" primIntToDouble :: Int -> Double
foreign import prim "intShow" primIntShow :: Int -> String
foreign import prim "integerToInt" primIntegerToInt :: Integer -> Int

foreign import prim "integerAdd" primIntegerAdd :: Integer -> Integer -> Integer
foreign import prim "integerSub" primIntegerSub :: Integer -> Integer -> Integer
foreign import prim "integerMul" primIntegerMul :: Integer -> Integer -> Integer
foreign import prim "integerNegate" primIntegerNegate :: Integer -> Integer
foreign import prim "integerAbs" primIntegerAbs :: Integer -> Integer
foreign import prim "integerSignum" primIntegerSignum :: Integer -> Integer
foreign import prim "integerQuot" primIntegerQuot :: Integer -> Integer -> Integer
foreign import prim "integerRem" primIntegerRem :: Integer -> Integer -> Integer
foreign import prim "integerDiv" primIntegerDiv :: Integer -> Integer -> Integer
foreign import prim "integerMod" primIntegerMod :: Integer -> Integer -> Integer
foreign import prim "integerEq" primIntegerEq :: Integer -> Integer -> Bool
foreign import prim "integerLt" primIntegerLt :: Integer -> Integer -> Bool
foreign import prim "integerLe" primIntegerLe :: Integer -> Integer -> Bool
foreign import prim "integerToDouble" primIntegerToDouble :: Integer -> Double
foreign import prim "integerShow" primIntegerShow :: Integer -> String

foreign import prim "doubleAdd" primDoubleAdd :: Double -> Double -> Double
foreign import prim "doubleSub" primDoubleSub :: Double -> Double -> Double
foreign import prim "doubleMul" primDoubleMul :: Double -> Double -> Double
foreign import prim "doubleDiv" primDoubleDiv :: Double -> Double -> Double
foreign import prim "doubleNegate" primDoubleNegate :: Double -> Double
foreign import prim "doubleEq" primDoubleEq :: Double -> Double -> Bool
foreign import prim "doubleLt" primDoubleLt :: Double -> Double -> Bool
foreign import prim "doubleLe" primDoubleLe :: Double -> Double -> Bool
foreign import prim "doubleShow" primDoubleShow :: Double -> String
foreign import prim "doubleSqrt" primDoubleSqrt :: Double -> Double
foreign import prim "doubleExp" primDoubleExp :: Double -> Double
foreign import prim "doubleLog" primDoubleLog :: Double -> Double
foreign import prim "doublePow" primDoublePow :: Double -> Double -> Double
foreign import prim "doubleSin" primDoubleSin :: Double -> Double
foreign import prim "doubleCos" primDoubleCos :: Double -> Double
foreign import prim "doubleTan" primDoubleTan :: Double -> Double
foreign import prim "doubleTruncate" primDoubleTruncate :: Double -> Integer
foreign import prim "doubleFloor" primDoubleFloor :: Double -> Integer
foreign import prim "doubleCeiling" primDoubleCeiling :: Double -> Integer

foreign import prim "charOrd" primCharOrd :: Char -> Int
foreign import prim "charChr" primCharChr :: Int -> Char
foreign import prim "charEq" primCharEq :: Char -> Char -> Bool
foreign import prim "charLt" primCharLt :: Char -> Char -> Bool
foreign import prim "charLe" primCharLe :: Char -> Char -> Bool

foreign import prim "error" primError :: String -> a
foreign import prim "putStr" primPutStr :: String -> World -> (# World, () #)
foreign import prim "putErr" primPutErr :: String -> World -> (# World, () #)
foreign import prim "getLine" primGetLine :: World -> (# World, String #)
foreign import prim "getContents" primGetContents :: World -> (# World, String #)
foreign import prim "isEOF" primIsEOF :: World -> (# World, Bool #)
foreign import prim "readFile" primReadFile :: String -> World -> (# World, String #)
foreign import prim "exitWith" primExitWith :: Int -> World -> (# World, () #)

-- ----- basic combinators --------------------------------------------------------

id :: a -> a
id x = x

const :: a -> b -> a
const x _ = x

(.) :: (b -> c) -> (a -> b) -> a -> c
(.) f g = \x -> f (g x)

flip :: (a -> b -> c) -> b -> a -> c
flip f x y = f y x

($) :: (a -> b) -> a -> b
f $ x = f x

($!) :: (a -> b) -> a -> b
f $! x = x `seq` f x

otherwise :: Bool
otherwise = True

error :: String -> a
error s = primError s

undefined :: a
undefined = error "Prelude.undefined"

-- The call-site versions used in user code (see the checker).
foreign import prim "errorLoc" primErrorLoc :: String -> String -> a

errorAt :: String -> String -> a
errorAt site msg = primErrorLoc msg site

undefinedAt :: String -> a
undefinedAt site = primErrorLoc "Prelude.undefined" site

asTypeOf :: a -> a -> a
asTypeOf x _ = x

until :: (a -> Bool) -> (a -> a) -> a -> a
until p f x = if p x then x else until p f (f x)

-- ----- Bool ----------------------------------------------------------------------

(&&) :: Bool -> Bool -> Bool
True && x = x
False && _ = False

(||) :: Bool -> Bool -> Bool
True || _ = True
False || x = x

not :: Bool -> Bool
not True = False
not False = True

bool :: a -> a -> Bool -> a
bool f _ False = f
bool _ t True = t

-- ----- Eq and Ord ------------------------------------------------------------------

class Eq a where
  (==) :: a -> a -> Bool
  (/=) :: a -> a -> Bool
  x /= y = not (x == y)
  x == y = not (x /= y)

class Eq a => Ord a where
  compare :: a -> a -> Ordering
  (<) :: a -> a -> Bool
  (<=) :: a -> a -> Bool
  (>) :: a -> a -> Bool
  (>=) :: a -> a -> Bool
  max :: a -> a -> a
  min :: a -> a -> a
  compare x y = if x == y then EQ else if x <= y then LT else GT
  x < y = case compare x y of { LT -> True; _ -> False }
  x <= y = case compare x y of { GT -> False; _ -> True }
  x > y = case compare x y of { GT -> True; _ -> False }
  x >= y = case compare x y of { LT -> False; _ -> True }
  max x y = if x <= y then y else x
  min x y = if x <= y then x else y

instance Eq Int where
  x == y = primIntEq x y

instance Ord Int where
  x < y = primIntLt x y
  x <= y = primIntLe x y
  x > y = primIntLt y x
  x >= y = primIntLe y x
  compare x y = if primIntEq x y then EQ else if primIntLt x y then LT else GT

instance Eq Integer where
  x == y = primIntegerEq x y

instance Ord Integer where
  x < y = primIntegerLt x y
  x <= y = primIntegerLe x y
  x > y = primIntegerLt y x
  x >= y = primIntegerLe y x
  compare x y = if primIntegerEq x y then EQ else if primIntegerLt x y then LT else GT

instance Eq Double where
  x == y = primDoubleEq x y

instance Ord Double where
  x < y = primDoubleLt x y
  x <= y = primDoubleLe x y
  x > y = primDoubleLt y x
  x >= y = primDoubleLe y x

instance Eq Char where
  x == y = primCharEq x y

instance Ord Char where
  x < y = primCharLt x y
  x <= y = primCharLe x y
  x > y = primCharLt y x
  x >= y = primCharLe y x
  compare x y = if primCharEq x y then EQ else if primCharLt x y then LT else GT

instance Eq Bool where
  True == True = True
  False == False = True
  _ == _ = False

instance Ord Bool where
  compare False True = LT
  compare True False = GT
  compare _ _ = EQ

instance Eq Ordering where
  LT == LT = True
  EQ == EQ = True
  GT == GT = True
  _ == _ = False

instance Ord Ordering where
  compare x y = compareInt (ordTag x) (ordTag y)

ordTag :: Ordering -> Int
ordTag LT = 0
ordTag EQ = 1
ordTag GT = 2

compareInt :: Int -> Int -> Ordering
compareInt x y = if primIntEq x y then EQ else if primIntLt x y then LT else GT

instance Eq () where
  _ == _ = True

instance Ord () where
  compare _ _ = EQ

instance Eq a => Eq (Maybe a) where
  Nothing == Nothing = True
  Just x == Just y = x == y
  _ == _ = False

instance Ord a => Ord (Maybe a) where
  compare Nothing Nothing = EQ
  compare Nothing (Just _) = LT
  compare (Just _) Nothing = GT
  compare (Just x) (Just y) = compare x y

instance (Eq a, Eq b) => Eq (Either a b) where
  Left x == Left y = x == y
  Right x == Right y = x == y
  _ == _ = False

instance (Ord a, Ord b) => Ord (Either a b) where
  compare (Left x) (Left y) = compare x y
  compare (Left _) (Right _) = LT
  compare (Right _) (Left _) = GT
  compare (Right x) (Right y) = compare x y

instance Eq a => Eq [a] where
  [] == [] = True
  (x : xs) == (y : ys) = x == y && xs == ys
  _ == _ = False

instance Ord a => Ord [a] where
  compare [] [] = EQ
  compare [] (_ : _) = LT
  compare (_ : _) [] = GT
  compare (x : xs) (y : ys) = case compare x y of
    EQ -> compare xs ys
    other -> other

instance (Eq a, Eq b) => Eq (a, b) where
  (a, b) == (c, d) = a == c && b == d

instance (Ord a, Ord b) => Ord (a, b) where
  compare (a, b) (c, d) = case compare a c of
    EQ -> compare b d
    other -> other

instance (Eq a, Eq b, Eq c) => Eq (a, b, c) where
  (a, b, c) == (d, e, f) = a == d && b == e && c == f

instance (Ord a, Ord b, Ord c) => Ord (a, b, c) where
  compare (a, b, c) (d, e, f) = case compare a d of
    EQ -> case compare b e of
      EQ -> compare c f
      other -> other
    other -> other

comparing :: Ord b => (a -> b) -> a -> a -> Ordering
comparing f x y = compare (f x) (f y)

-- ----- Enum and Bounded ------------------------------------------------------------

class Enum a where
  succ :: a -> a
  pred :: a -> a
  toEnum :: Int -> a
  fromEnum :: a -> Int
  enumFrom :: a -> [a]
  enumFromThen :: a -> a -> [a]
  enumFromTo :: a -> a -> [a]
  enumFromThenTo :: a -> a -> a -> [a]
  succ x = toEnum (fromEnum x + 1)
  pred x = toEnum (fromEnum x - 1)
  enumFrom x = map toEnum (enumFromInt (fromEnum x))
  enumFromThen x y = map toEnum (enumFromThenInt (fromEnum x) (fromEnum y))
  enumFromTo x y = map toEnum (enumFromToInt (fromEnum x) (fromEnum y))
  enumFromThenTo x y z = map toEnum (enumFromThenToInt (fromEnum x) (fromEnum y) (fromEnum z))

class Bounded a where
  minBound :: a
  maxBound :: a

enumFromInt :: Int -> [Int]
enumFromInt n = n : enumFromInt (n + 1)

enumFromThenInt :: Int -> Int -> [Int]
enumFromThenInt n m = n : enumFromThenInt m (m + m - n)

enumFromToInt :: Int -> Int -> [Int]
enumFromToInt n m = if n > m then [] else n : enumFromToInt (n + 1) m

enumFromThenToInt :: Int -> Int -> Int -> [Int]
enumFromThenToInt n m k =
  if m >= n
    then (if n > k then [] else n : enumFromThenToInt m (m + m - n) k)
    else (if n < k then [] else n : enumFromThenToInt m (m + m - n) k)

instance Enum Int where
  succ x = x + 1
  pred x = x - 1
  toEnum x = x
  fromEnum x = x
  enumFrom x = enumFromInt x
  enumFromThen x y = enumFromThenInt x y
  enumFromTo x y = enumFromToInt x y
  enumFromThenTo x y z = enumFromThenToInt x y z

instance Enum Integer where
  succ x = x + 1
  pred x = x - 1
  toEnum x = primIntToInteger x
  fromEnum x = primIntegerToInt x
  enumFrom x = x : enumFrom (x + 1)
  enumFromTo x y = if x > y then [] else x : enumFromTo (x + 1) y
  enumFromThen x y = x : enumFromThen y (y + y - x)
  enumFromThenTo x y z = if y >= x
    then (if x > z then [] else x : enumFromThenTo y (y + y - x) z)
    else (if x < z then [] else x : enumFromThenTo y (y + y - x) z)

instance Enum Char where
  toEnum n = primCharChr n
  fromEnum c = primCharOrd c
  enumFromTo x y = map primCharChr (enumFromToInt (primCharOrd x) (primCharOrd y))

instance Enum Bool where
  toEnum 0 = False
  toEnum _ = True
  fromEnum False = 0
  fromEnum True = 1
  enumFrom x = enumFromTo x True
  enumFromTo x y = map toEnum (enumFromToInt (fromEnum x) (fromEnum y))

instance Enum Ordering where
  toEnum n = if n == 0 then LT else if n == 1 then EQ else GT
  fromEnum x = ordTag x
  enumFrom x = enumFromTo x GT
  enumFromTo x y = map toEnum (enumFromToInt (fromEnum x) (fromEnum y))

instance Enum () where
  toEnum _ = ()
  fromEnum _ = 0

instance Bounded Int where
  minBound = primIntNegate 9223372036854775807 - 1
  maxBound = 9223372036854775807

instance Bounded Char where
  minBound = '\0'
  maxBound = '\1114111'

instance Bounded Bool where
  minBound = False
  maxBound = True

instance Bounded Ordering where
  minBound = LT
  maxBound = GT

instance Bounded () where
  minBound = ()
  maxBound = ()

-- ----- numbers ----------------------------------------------------------------------

class Num a where
  (+) :: a -> a -> a
  (-) :: a -> a -> a
  (*) :: a -> a -> a
  negate :: a -> a
  abs :: a -> a
  signum :: a -> a
  fromInteger :: Integer -> a
  x - y = x + negate y
  negate x = fromInteger 0 - x

class (Num a, Ord a) => Real a where
  toRational :: a -> Rational

class (Real a, Enum a) => Integral a where
  quot :: a -> a -> a
  rem :: a -> a -> a
  div :: a -> a -> a
  mod :: a -> a -> a
  quotRem :: a -> a -> (a, a)
  divMod :: a -> a -> (a, a)
  toInteger :: a -> Integer
  quotRem x y = (quot x y, rem x y)
  divMod x y = (div x y, mod x y)

class Num a => Fractional a where
  (/) :: a -> a -> a
  recip :: a -> a
  fromRational :: Rational -> a
  recip x = fromInteger 1 / x

class (Real a, Fractional a) => RealFrac a where
  properFraction :: Integral b => a -> (b, a)
  truncate :: Integral b => a -> b
  round :: Integral b => a -> b
  ceiling :: Integral b => a -> b
  floor :: Integral b => a -> b

class Fractional a => Floating a where
  pi :: a
  exp :: a -> a
  log :: a -> a
  sqrt :: a -> a
  sin :: a -> a
  cos :: a -> a
  tan :: a -> a
  (**) :: a -> a -> a

instance Num Int where
  x + y = primIntAdd x y
  x - y = primIntSub x y
  x * y = primIntMul x y
  negate x = primIntNegate x
  abs x = if x < 0 then primIntNegate x else x
  signum x = if x < 0 then negate 1 else if x == 0 then 0 else 1
  fromInteger n = primIntegerToInt n

instance Real Int where
  toRational x = primIntToInteger x :% 1

instance Integral Int where
  quot x y = primIntQuot x y
  rem x y = primIntRem x y
  div x y = primIntDiv x y
  mod x y = primIntMod x y
  quotRem x y = (primIntQuot x y, primIntRem x y)
  divMod x y = (primIntDiv x y, primIntMod x y)
  toInteger x = primIntToInteger x

instance Num Integer where
  x + y = primIntegerAdd x y
  x - y = primIntegerSub x y
  x * y = primIntegerMul x y
  negate x = primIntegerNegate x
  abs x = primIntegerAbs x
  signum x = primIntegerSignum x
  fromInteger n = n

instance Real Integer where
  toRational x = x :% 1

instance Integral Integer where
  quot x y = primIntegerQuot x y
  rem x y = primIntegerRem x y
  div x y = primIntegerDiv x y
  mod x y = primIntegerMod x y
  toInteger x = x

instance Num Double where
  x + y = primDoubleAdd x y
  x - y = primDoubleSub x y
  x * y = primDoubleMul x y
  negate x = primDoubleNegate x
  abs x = if x < 0 then primDoubleNegate x else x
  signum x = if x < 0 then negate 1 else if x == 0 then 0 else 1
  fromInteger n = primIntegerToDouble n

instance Real Double where
  toRational x = doubleToRational x

instance Fractional Double where
  x / y = primDoubleDiv x y
  fromRational (n :% d) = primIntegerToDouble n / primIntegerToDouble d

instance RealFrac Double where
  properFraction x = let n = primDoubleTruncate x in (fromInteger n, x - primIntegerToDouble n)
  truncate x = fromInteger (primDoubleTruncate x)
  floor x = fromInteger (primDoubleFloor x)
  ceiling x = fromInteger (primDoubleCeiling x)
  round x = fromInteger (roundHalfEven x)

roundHalfEven :: Double -> Integer
roundHalfEven x =
  let f = primDoubleFloor x
      diff = x - primIntegerToDouble f
  in if diff < 0.5 then f
     else if diff > 0.5 then f + 1
     else if even f then f else f + 1

foreign import prim "doubleToRatio" primDoubleToRatio :: Double -> Rational

doubleToRational :: Double -> Rational
doubleToRational x = primDoubleToRatio x

instance Floating Double where
  pi = 3.141592653589793
  exp x = primDoubleExp x
  log x = primDoubleLog x
  sqrt x = primDoubleSqrt x
  sin x = primDoubleSin x
  cos x = primDoubleCos x
  tan x = primDoubleTan x
  x ** y = primDoublePow x y

class (RealFrac a, Floating a) => RealFloat a where
  atan2 :: a -> a -> a
  isNaN :: a -> Bool
  isInfinite :: a -> Bool
  isNegativeZero :: a -> Bool
  floatDigits :: a -> Int
  floatRadix :: a -> Integer
  floatRadix _ = 2

instance RealFloat Double where
  atan2 y x = primDoubleAtan2 y x
  isNaN x = primDoubleIsNaN x
  isInfinite x = primDoubleIsInfinite x
  isNegativeZero x = x == 0 && primDoubleShow x == "-0.0"
  floatDigits _ = 53

instance RealFloat Float where
  atan2 y x = primFloatFromDouble (primDoubleAtan2 (primFloatToDouble y) (primFloatToDouble x))
  isNaN x = primDoubleIsNaN (primFloatToDouble x)
  isInfinite x = primDoubleIsInfinite (primFloatToDouble x)
  isNegativeZero x = primFloatShow x == "-0.0"
  floatDigits _ = 24

foreign import prim "doubleAtan2" primDoubleAtan2 :: Double -> Double -> Double
foreign import prim "doubleIsNaN" primDoubleIsNaN :: Double -> Bool
foreign import prim "doubleIsInfinite" primDoubleIsInfinite :: Double -> Bool

-- Ratio and Rational.

instance Eq a => Eq (Ratio a) where
  (a :% b) == (c :% d) = a == c && b == d

instance Integral a => Ord (Ratio a) where
  compare (a :% b) (c :% d) = compare (a * d) (c * b)

instance Integral a => Num (Ratio a) where
  (a :% b) + (c :% d) = reduce (a * d + c * b) (b * d)
  (a :% b) - (c :% d) = reduce (a * d - c * b) (b * d)
  (a :% b) * (c :% d) = reduce (a * c) (b * d)
  negate (a :% b) = negate a :% b
  abs (a :% b) = abs a :% b
  signum (a :% _) = signum a :% 1
  fromInteger n = fromInteger n :% 1

instance Integral a => Fractional (Ratio a) where
  (a :% b) / (c :% d) = reduce (a * d) (b * c)
  fromRational (n :% d) = fromInteger n :% fromInteger d

instance Integral a => Real (Ratio a) where
  toRational (a :% b) = toInteger a :% toInteger b

instance Integral a => RealFrac (Ratio a) where
  properFraction (a :% b) = let (q, r) = quotRem a b in (fromIntegral q, r :% b)
  truncate (a :% b) = fromIntegral (quot a b)
  floor (a :% b) = fromIntegral (div a b)
  ceiling (a :% b) = fromIntegral (negate (div (negate a) b))
  round x =
    let (n, r) = properFraction x
        m = if r < fromInteger 0 then n - 1 else n + 1
        half = fromInteger 1 / fromInteger 2
        d = abs r - half
    in if d < fromInteger 0 then n
       else if d == fromInteger 0 then (if even n then n else m)
       else m

reduce :: Integral a => a -> a -> Ratio a
reduce x y = let g = gcd x y in (x `quot` g) :% (y `quot` g)

(%) :: Integral a => a -> a -> Ratio a
x % y = reduce (x * signum y) (abs y)

numerator :: Ratio a -> a
numerator (a :% _) = a

denominator :: Ratio a -> a
denominator (_ :% b) = b

instance Show a => Show (Ratio a) where
  showsPrec p (a :% b) = showParen (p > 7) (showsPrec 8 a . showString " % " . showsPrec 8 b)

fromIntegral :: Integral a => Num b => a -> b
fromIntegral x = fromInteger (toInteger x)

realToFrac :: Real a => Fractional b => a -> b
realToFrac x = fromRational (toRational x)

even :: Integral a => a -> Bool
even n = rem n 2 == 0

odd :: Integral a => a -> Bool
odd n = not (even n)

subtract :: Num a => a -> a -> a
subtract x y = y - x

gcd :: Integral a => a -> a -> a
gcd x y = gcd' (abs x) (abs y)
  where
    gcd' a b = if b == 0 then a else gcd' b (rem a b)

lcm :: Integral a => a -> a -> a
lcm x y = if x == 0 || y == 0 then 0 else abs ((x `quot` gcd x y) * y)

(^) :: Num a => Integral b => a -> b -> a
x ^ n = if n == 0 then 1 else if even n then (let h = x ^ (n `quot` 2) in h * h) else x * x ^ (n - 1)

(^^) :: Fractional a => Integral b => a -> b -> a
x ^^ n = if n >= 0 then x ^ n else recip (x ^ negate n)

-- ----- Show -------------------------------------------------------------------------

class Show a where
  showsPrec :: Int -> a -> ShowS
  show :: a -> String
  showList :: [a] -> ShowS
  showsPrec _ x s = show x ++ s
  show x = showsPrec 0 x ""
  showList ls s = showListWith shows ls s

showListWith :: (a -> ShowS) -> [a] -> ShowS
showListWith _ [] s = "[]" ++ s
showListWith showx (x : xs) s = '[' : showx x (showl xs)
  where
    showl [] = ']' : s
    showl (y : ys) = ',' : showx y (showl ys)

shows :: Show a => a -> ShowS
shows = showsPrec 0

showChar :: Char -> ShowS
showChar c s = c : s

showString :: String -> ShowS
showString str s = str ++ s

showParen :: Bool -> ShowS -> ShowS
showParen b p = if b then showChar '(' . p . showChar ')' else p

instance Show Int where
  showsPrec p n r = if n < 0 && p > 6
    then '(' : primIntShow n ++ (')' : r)
    else primIntShow n ++ r
  show n = primIntShow n

instance Show Integer where
  showsPrec p n r = if n < 0 && p > 6
    then '(' : primIntegerShow n ++ (')' : r)
    else primIntegerShow n ++ r
  show n = primIntegerShow n

instance Show Double where
  showsPrec p n r = if (n < 0 || isNegativeZero n) && p > 6
    then '(' : primDoubleShow n ++ (')' : r)
    else primDoubleShow n ++ r
  show n = primDoubleShow n



instance Show Bool where
  showsPrec _ True = showString "True"
  showsPrec _ False = showString "False"

instance Show Ordering where
  showsPrec _ LT = showString "LT"
  showsPrec _ EQ = showString "EQ"
  showsPrec _ GT = showString "GT"

instance Show () where
  showsPrec _ _ = showString "()"

instance Show Char where
  showsPrec _ '\'' = showString "'\\''"
  showsPrec _ c = showChar '\'' . showLitChar c . showChar '\''
  showList cs = showChar '"' . showLitString cs . showChar '"'

showLitString :: String -> ShowS
showLitString [] s = s
showLitString ('"' : cs) s = showString "\\\"" (showLitString cs s)
showLitString (c : cs) s = showLitChar c (showLitString cs s)

showLitChar :: Char -> ShowS
showLitChar c s
  | primCharOrd c > 127 = showChar '\\' (protectEsc isDigitC (showString (primIntShow (primCharOrd c))) s)
  | c == '\DEL' = showString "\\DEL" s
  | c == '\\' = showString "\\\\" s
  | primCharOrd c >= 32 = showChar c s
  | c == '\a' = showString "\\a" s
  | c == '\b' = showString "\\b" s
  | c == '\f' = showString "\\f" s
  | c == '\n' = showString "\\n" s
  | c == '\r' = showString "\\r" s
  | c == '\t' = showString "\\t" s
  | c == '\v' = showString "\\v" s
  | c == '\SO' = protectEsc (\x -> x == 'H') (showString "\\SO") s
  | otherwise = showChar '\\' (showString (asciiTab !! primCharOrd c) s)

isDigitC :: Char -> Bool
isDigitC c = c >= '0' && c <= '9'

protectEsc :: (Char -> Bool) -> ShowS -> ShowS
protectEsc p f = f . cont
  where
    cont s@(c : _) = if p c then "\\&" ++ s else s
    cont [] = []

asciiTab :: [String]
asciiTab =
  ["NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL",
   "BS", "HT", "LF", "VT", "FF", "CR", "SO", "SI",
   "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB",
   "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS", "US", "SP"]

instance Show a => Show [a] where
  showsPrec _ = showList

instance Show a => Show (Maybe a) where
  showsPrec _ Nothing = showString "Nothing"
  showsPrec d (Just x) = showParen (d > 10) (showString "Just " . showsPrec 11 x)

instance (Show a, Show b) => Show (Either a b) where
  showsPrec d (Left x) = showParen (d > 10) (showString "Left " . showsPrec 11 x)
  showsPrec d (Right x) = showParen (d > 10) (showString "Right " . showsPrec 11 x)

instance (Show a, Show b) => Show (a, b) where
  showsPrec _ (a, b) = showChar '(' . shows a . showChar ',' . shows b . showChar ')'

instance (Show a, Show b, Show c) => Show (a, b, c) where
  showsPrec _ (a, b, c) = showChar '(' . shows a . showChar ',' . shows b . showChar ',' . shows c . showChar ')'

-- ----- Functor, Applicative, Monad --------------------------------------------------

class Functor f where
  fmap :: (a -> b) -> f a -> f b
  (<$) :: a -> f b -> f a
  x <$ m = fmap (const x) m

class Functor f => Applicative f where
  pure :: a -> f a
  (<*>) :: f (a -> b) -> f a -> f b
  (*>) :: f a -> f b -> f b
  (<*) :: f a -> f b -> f a
  a *> b = fmap (\_ y -> y) a <*> b
  a <* b = fmap (\x _ -> x) a <*> b

class Applicative m => Monad m where
  (>>=) :: m a -> (a -> m b) -> m b
  (>>) :: m a -> m b -> m b
  return :: a -> m a
  m >> k = m >>= \_ -> k
  return x = pure x

class Monad m => MonadFail m where
  fail :: String -> m a

-- Monads with a fixed point operator; `rec` blocks and `mdo` are desugared to `mfix`.
class Monad m => MonadFix m where
  mfix :: (a -> m a) -> m a

foreign import prim "newRef" primFixNewRef :: a -> World -> (# World, (Ref a) #)
foreign import prim "readRef" primFixReadRef :: Ref a -> World -> (# World, a #)
foreign import prim "writeRef" primFixWriteRef :: Ref a -> a -> World -> (# World, () #)
foreign import prim "unsafeRunIO" primFixRunIO :: IO a -> a

-- The result is read lazily from a cell that is filled once the action has finished.
fixIO :: (a -> IO a) -> IO a
fixIO k = IO (\w -> case primFixNewRef (error "cyclic evaluation in fixIO") w of
  (# w1, ref #) ->
    let ans = primFixRunIO (IO (primFixReadRef ref))
    in case k ans of
         IO m -> case m w1 of
           (# w2, result #) -> case primFixWriteRef ref result w2 of
             (# w3, _ #) -> (# w3, result #))

instance MonadFix IO where
  mfix = fixIO

instance MonadFix Maybe where
  mfix f = let a = f (unJustFix a) in a
    where
      unJustFix (Just x) = x
      unJustFix Nothing = error "mfix Maybe: Nothing"

instance MonadFix (Either e) where
  mfix f = let a = f (unRightFix a) in a
    where
      unRightFix (Right x) = x
      unRightFix (Left _) = error "mfix Either: Left"

instance MonadFix [] where
  mfix f = case (let r = f (headFix r) in r) of
    [] -> []
    (x : _) -> x : mfix (tailFix . f)
    where
      headFix (x : _) = x
      headFix [] = error "mfix []: empty list"
      tailFix (_ : xs) = xs
      tailFix [] = []

(<$>) :: Functor f => (a -> b) -> f a -> f b
f <$> x = fmap f x

(=<<) :: Monad m => (a -> m b) -> m a -> m b
f =<< m = m >>= f

liftA2 :: Applicative f => (a -> b -> c) -> f a -> f b -> f c
liftA2 f x y = fmap f x <*> y

instance Functor [] where
  fmap = map

instance Applicative [] where
  pure x = [x]
  fs <*> xs = concatMap (\f -> map f xs) fs

instance Monad [] where
  xs >>= f = concatMap f xs

instance MonadFail [] where
  fail _ = []

instance Functor Maybe where
  fmap _ Nothing = Nothing
  fmap f (Just x) = Just (f x)

instance Applicative Maybe where
  pure x = Just x
  Just f <*> m = fmap f m
  Nothing <*> _ = Nothing

instance Monad Maybe where
  Nothing >>= _ = Nothing
  Just x >>= k = k x

instance MonadFail Maybe where
  fail _ = Nothing

instance Functor (Either e) where
  fmap _ (Left e) = Left e
  fmap f (Right x) = Right (f x)

instance Applicative (Either e) where
  pure x = Right x
  Left e <*> _ = Left e
  Right f <*> r = fmap f r

instance Monad (Either e) where
  Left e >>= _ = Left e
  Right x >>= k = k x

instance Functor IO where
  fmap f (IO m) = IO (\w -> case m w of (# w1, a #) -> (# w1, f a #))

instance Applicative IO where
  pure x = IO (\w -> (# w, x #))
  IO mf <*> IO mx = IO (\w -> case mf w of (# w1, f #) -> case mx w1 of (# w2, x #) -> (# w2, f x #))

instance Monad IO where
  IO m >>= k = IO (\w -> case m w of (# w1, a #) -> unIO (k a) w1)

instance MonadFail IO where
  fail s = ioError (userError s)

unIO :: IO a -> World -> (# World, a #)
unIO (IO m) = m

-- ----- Semigroup and Monoid ---------------------------------------------------------

class Semigroup a where
  (<>) :: a -> a -> a

class Semigroup a => Monoid a where
  mempty :: a
  mappend :: a -> a -> a
  mconcat :: [a] -> a
  mappend = (<>)
  mconcat xs = foldr mappend mempty xs

instance Semigroup [a] where
  xs <> ys = xs ++ ys

instance Monoid [a] where
  mempty = []

instance Semigroup Ordering where
  LT <> _ = LT
  EQ <> y = y
  GT <> _ = GT

instance Monoid Ordering where
  mempty = EQ

instance Semigroup a => Semigroup (Maybe a) where
  Nothing <> b = b
  a <> Nothing = a
  Just a <> Just b = Just (a <> b)

instance Semigroup a => Monoid (Maybe a) where
  mempty = Nothing


-- ----- Foldable and Traversable ------------------------------------------------------

class Foldable t where
  foldMap :: Monoid m => (a -> m) -> t a -> m
  foldr :: (a -> b -> b) -> b -> t a -> b
  foldl :: (b -> a -> b) -> b -> t a -> b
  foldl' :: (b -> a -> b) -> b -> t a -> b
  foldr1 :: (a -> a -> a) -> t a -> a
  foldl1 :: (a -> a -> a) -> t a -> a
  toList :: t a -> [a]
  null :: t a -> Bool
  length :: t a -> Int
  elem :: Eq a => a -> t a -> Bool
  maximum :: Ord a => t a -> a
  minimum :: Ord a => t a -> a
  sum :: Num a => t a -> a
  product :: Num a => t a -> a
  foldMap f t = foldr (\x acc -> f x <> acc) mempty t
  foldl f z t = listFoldl f z (toList t)
  foldl' f z t = listFoldl' f z (toList t)
  foldr1 f t = listFoldr1 f (toList t)
  foldl1 f t = listFoldl1 f (toList t)
  toList t = foldr (:) [] t
  null t = listNull (toList t)
  length t = listLength (toList t)
  elem x t = listElem x (toList t)
  maximum t = listMaximum (toList t)
  minimum t = listMinimum (toList t)
  sum t = listSum (toList t)
  product t = listProduct (toList t)

instance Foldable [] where
  foldr = listFoldr
  foldl = listFoldl
  foldl' = listFoldl'
  foldr1 = listFoldr1
  foldl1 = listFoldl1
  toList xs = xs
  null = listNull
  length = listLength
  elem = listElem
  maximum = listMaximum
  minimum = listMinimum
  sum = listSum
  product = listProduct

instance Foldable Maybe where
  foldr _ z Nothing = z
  foldr f z (Just x) = f x z
  toList Nothing = []
  toList (Just x) = [x]

instance Foldable (Either e) where
  foldr _ z (Left _) = z
  foldr f z (Right x) = f x z

class (Functor t, Foldable t) => Traversable t where
  traverse :: Applicative f => (a -> f b) -> t a -> f (t b)
  sequenceA :: Applicative f => t (f a) -> f (t a)
  mapM :: Monad m => (a -> m b) -> t a -> m (t b)
  sequence :: Monad m => t (m a) -> m (t a)
  sequenceA xs = traverse id xs
  mapM f xs = traverse f xs
  sequence xs = sequenceA xs

instance Traversable [] where
  traverse f xs = listFoldr (\x acc -> liftA2 (:) (f x) acc) (pure []) xs

instance Traversable Maybe where
  traverse _ Nothing = pure Nothing
  traverse f (Just x) = fmap Just (f x)

instance Traversable (Either e) where
  traverse _ (Left e) = pure (Left e)
  traverse f (Right x) = fmap Right (f x)

-- ----- lists ---------------------------------------------------------------------

map :: (a -> b) -> [a] -> [b]
map _ [] = []
map f (x : xs) = f x : map f xs

(++) :: [a] -> [a] -> [a]
[] ++ ys = ys
(x : xs) ++ ys = x : (xs ++ ys)

filter :: (a -> Bool) -> [a] -> [a]
filter _ [] = []
filter p (x : xs) = if p x then x : filter p xs else filter p xs

head :: [a] -> a
head (x : _) = x
head [] = errorEmptyList "head"

tail :: [a] -> [a]
tail (_ : xs) = xs
tail [] = errorEmptyList "tail"

last :: [a] -> a
last [x] = x
last (_ : xs) = last xs
last [] = errorEmptyList "last"

init :: [a] -> [a]
init [x] = []
init (x : xs) = x : init xs
init [] = errorEmptyList "init"

errorEmptyList :: String -> a
errorEmptyList fun = error ("Prelude." ++ fun ++ ": empty list")

listNull :: [a] -> Bool
listNull [] = True
listNull _ = False

listLength :: [a] -> Int
listLength xs = lenAcc xs 0

lenAcc :: [a] -> Int -> Int
lenAcc [] n = n
lenAcc (_ : ys) n = let m = n + 1 in m `seq` lenAcc ys m

listFoldl :: (b -> a -> b) -> b -> [a] -> b
listFoldl f z [] = z
listFoldl f z (x : xs) = listFoldl f (f z x) xs

listFoldl' :: (b -> a -> b) -> b -> [a] -> b
listFoldl' f z [] = z
listFoldl' f z (x : xs) = let z' = f z x in z' `seq` listFoldl' f z' xs

listFoldl1 :: (a -> a -> a) -> [a] -> a
listFoldl1 f (x : xs) = listFoldl f x xs
listFoldl1 _ [] = errorEmptyList "foldl1"

listFoldr :: (a -> b -> b) -> b -> [a] -> b
listFoldr f z [] = z
listFoldr f z (x : xs) = f x (listFoldr f z xs)

listFoldr1 :: (a -> a -> a) -> [a] -> a
listFoldr1 f [x] = x
listFoldr1 f (x : xs) = f x (listFoldr1 f xs)
listFoldr1 _ [] = errorEmptyList "foldr1"

listSum :: Num a => [a] -> a
listSum xs = listFoldl' (+) 0 xs

listProduct :: Num a => [a] -> a
listProduct xs = listFoldl' (*) 1 xs

listMaximum :: Ord a => [a] -> a
listMaximum [] = errorEmptyList "maximum"
listMaximum xs = listFoldl1 max xs

listMinimum :: Ord a => [a] -> a
listMinimum [] = errorEmptyList "minimum"
listMinimum xs = listFoldl1 min xs

listAnd :: [Bool] -> Bool
listAnd xs = listFoldr (&&) True xs

listOr :: [Bool] -> Bool
listOr xs = listFoldr (||) False xs

any :: Foldable t => (a -> Bool) -> t a -> Bool
any p xs = foldr (\x acc -> p x || acc) False xs

all :: Foldable t => (a -> Bool) -> t a -> Bool
all p xs = foldr (\x acc -> p x && acc) True xs

listConcat :: [[a]] -> [a]
listConcat xss = listFoldr (++) [] xss

and :: Foldable t => t Bool -> Bool
and xs = foldr (&&) True xs

or :: Foldable t => t Bool -> Bool
or xs = foldr (||) False xs

concat :: Foldable t => t [a] -> [a]
concat xss = foldr (++) [] xss

concatMap :: Foldable t => (a -> [b]) -> t a -> [b]
concatMap f xs = foldr (\x acc -> f x ++ acc) [] xs

listElem :: Eq a => a -> [a] -> Bool
listElem _ [] = False
listElem x (y : ys) = x == y || listElem x ys

notElem :: (Foldable t, Eq a) => a -> t a -> Bool
notElem x ys = not (elem x ys)

(!!) :: [a] -> Int -> a
xs !! n = if n < 0 then error "Prelude.!!: negative index" else nth xs n

nth :: [a] -> Int -> a
nth [] _ = error "Prelude.!!: index too large"
nth (x : xs) n = if n == 0 then x else nth xs (n - 1)

reverse :: [a] -> [a]
reverse l = rev l []
  where
    rev [] a = a
    rev (x : xs) a = rev xs (x : a)

take :: Int -> [a] -> [a]
take n xs = if n <= 0 then [] else case xs of
  [] -> []
  (y : ys) -> y : take (n - 1) ys

drop :: Int -> [a] -> [a]
drop n xs = if n <= 0 then xs else case xs of
  [] -> []
  (_ : ys) -> drop (n - 1) ys

splitAt :: Int -> [a] -> ([a], [a])
splitAt n xs = (take n xs, drop n xs)

takeWhile :: (a -> Bool) -> [a] -> [a]
takeWhile _ [] = []
takeWhile p (x : xs) = if p x then x : takeWhile p xs else []

dropWhile :: (a -> Bool) -> [a] -> [a]
dropWhile _ [] = []
dropWhile p (x : xs) = if p x then dropWhile p xs else x : xs

span :: (a -> Bool) -> [a] -> ([a], [a])
span _ [] = ([], [])
span p (x : xs) = if p x
  then (let (ys, zs) = span p xs in (x : ys, zs))
  else ([], x : xs)

break :: (a -> Bool) -> [a] -> ([a], [a])
break p xs = span (not . p) xs

lines :: String -> [String]
lines "" = []
lines s = let (l, s') = break (\c -> c == '\n') s
          in l : (case s' of
                    [] -> []
                    (_ : s'') -> lines s'')

unlines :: [String] -> String
unlines [] = []
unlines (l : ls) = l ++ '\n' : unlines ls

words :: String -> [String]
words s = case dropWhile isSpaceC s of
  "" -> []
  s' -> let (w, s'') = break isSpaceC s' in w : words s''

unwords :: [String] -> String
unwords [] = ""
unwords [w] = w
unwords (w : ws) = w ++ ' ' : unwords ws

isSpaceC :: Char -> Bool
isSpaceC c = c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f' || c == '\v'

zip :: [a] -> [b] -> [(a, b)]
zip (a : as) (b : bs) = (a, b) : zip as bs
zip _ _ = []

zip3 :: [a] -> [b] -> [c] -> [(a, b, c)]
zip3 (a : as) (b : bs) (c : cs) = (a, b, c) : zip3 as bs cs
zip3 _ _ _ = []

zipWith :: (a -> b -> c) -> [a] -> [b] -> [c]
zipWith f (a : as) (b : bs) = f a b : zipWith f as bs
zipWith _ _ _ = []

zipWith3 :: (a -> b -> c -> d) -> [a] -> [b] -> [c] -> [d]
zipWith3 f (a : as) (b : bs) (c : cs) = f a b c : zipWith3 f as bs cs
zipWith3 _ _ _ _ = []

unzip :: [(a, b)] -> ([a], [b])
unzip ps = (map fst ps, map snd ps)

unzip3 :: [(a, b, c)] -> ([a], [b], [c])
unzip3 ps = (map (\(a, _, _) -> a) ps, map (\(_, b, _) -> b) ps, map (\(_, _, c) -> c) ps)

lookup :: Eq a => a -> [(a, b)] -> Maybe b
lookup _ [] = Nothing
lookup k ((a, b) : rest) = if k == a then Just b else lookup k rest

iterate :: (a -> a) -> a -> [a]
iterate f x = x : iterate f (f x)

repeat :: a -> [a]
repeat x = let xs = x : xs in xs

replicate :: Int -> a -> [a]
replicate n x = if n <= 0 then [] else x : replicate (n - 1) x

cycle :: [a] -> [a]
cycle [] = errorEmptyList "cycle"
cycle xs = let ys = xs ++ ys in ys

scanl :: (b -> a -> b) -> b -> [a] -> [b]
scanl f q ls = q : (case ls of
  [] -> []
  x : xs -> scanl f (f q x) xs)

scanl1 :: (a -> a -> a) -> [a] -> [a]
scanl1 f (x : xs) = scanl f x xs
scanl1 _ [] = []

scanr :: (a -> b -> b) -> b -> [a] -> [b]
scanr _ q0 [] = [q0]
scanr f q0 (x : xs) = case scanr f q0 xs of
  qs@(q : _) -> f x q : qs
  [] -> []

scanr1 :: (a -> a -> a) -> [a] -> [a]
scanr1 _ [] = []
scanr1 _ [x] = [x]
scanr1 f (x : xs) = case scanr1 f xs of
  qs@(q : _) -> f x q : qs
  [] -> []

-- ----- tuples and simple functions --------------------------------------------------

fst :: (a, b) -> a
fst (x, _) = x

snd :: (a, b) -> b
snd (_, y) = y

curry :: ((a, b) -> c) -> a -> b -> c
curry f x y = f (x, y)

uncurry :: (a -> b -> c) -> (a, b) -> c
uncurry f p = f (fst p) (snd p)

maybe :: b -> (a -> b) -> Maybe a -> b
maybe n _ Nothing = n
maybe _ f (Just x) = f x

either :: (a -> c) -> (b -> c) -> Either a b -> c
either f _ (Left x) = f x
either _ g (Right y) = g y

-- ----- characters ------------------------------------------------------------------

ord :: Char -> Int
ord c = primCharOrd c

chr :: Int -> Char
chr n = primCharChr n

-- ----- IO --------------------------------------------------------------------------

data IOError = IOError String

instance Show IOError where
  showsPrec _ (IOError s) = showString s

userError :: String -> IOError
userError s = IOError ("user error (" ++ s ++ ")")

ioError :: IOError -> IO a
ioError e = IO (\_ -> primError (show e))

putStr :: String -> IO ()
putStr s = IO (primPutStr s)

putStrLn :: String -> IO ()
putStrLn s = IO (primPutStr (s ++ "\n"))

putChar :: Char -> IO ()
putChar c = IO (primPutStr [c])

print :: Show a => a -> IO ()
print x = putStrLn (show x)

getLine :: IO String
getLine = IO primGetLine

getContents :: IO String
getContents = IO primGetContents

getChar :: IO Char
getChar = IO (\w -> case primGetLine w of (# w1, (c : _) #) -> (# w1, c #); (# _, [] #) -> primError "Prelude.getChar: end of file")

readFile :: FilePath -> IO String
readFile path = IO (primReadFile path)

interact :: (String -> String) -> IO ()
interact f = getContents >>= \s -> putStr (f s)

readLn :: Read a => IO a
readLn = getLine >>= \s -> return (read s)

isEOF :: IO Bool
isEOF = IO primIsEOF

sequence_ :: (Foldable t, Monad m) => t (m a) -> m ()
sequence_ ms = foldr (>>) (return ()) ms

listSequence :: Monad m => [m a] -> m [a]
listSequence [] = return []
listSequence (m : ms) = m >>= \x -> listSequence ms >>= \xs -> return (x : xs)

mapM_ :: (Foldable t, Monad m) => (a -> m b) -> t a -> m ()
mapM_ f xs = foldr (\x k -> f x >> k) (return ()) xs



-- ----- Read ---------------------------------------------------------------------------

class Read a where
  readsPrec :: Int -> ReadS a
  readList :: ReadS [a]
  readList = readListDefault

reads :: Read a => ReadS a
reads = readsPrec 0

-- Lexical analysis for derived instances: one token, after skipping spaces.
lex :: ReadS String
lex s = case dropWhile isSpaceC s of
  "" -> [("", "")]
  (c : cs)
    | isAlphaC c || c == '_' -> let (w, r) = span isIdentC (c : cs) in let (hs, r2) = span (== '#') r in [(w ++ hs, r2)]
    | isDigitC c -> let (w, r) = span isDigitC (c : cs) in [(w, r)]
    | c `elem` "()[]{},;`\"" -> [([c], cs)]
    | otherwise -> let (w, r) = span isSymC (c : cs) in if null w then [] else [(w, r)]
  where
    isIdentC x = isAlphaC x || isDigitC x || x == '_' || x == '\''
    isSymC x = x `elem` "!#$%&*+./<=>?@\\^|-~:"

readParen :: Bool -> ReadS a -> ReadS a
readParen b g = if b then mandatory else optional
  where
    optional r = g r ++ mandatory r
    mandatory r = [(x, u) | ("(", s) <- lex r, (x, t) <- optional s, (")", u) <- lex t]

read :: Read a => String -> a
read s = case [x | (x, rest) <- readsPrec 0 s, all isSpaceC rest] of
  (x : _) -> x
  [] -> error "Prelude.read: no parse"

readListDefault :: Read a => ReadS [a]
readListDefault s0 = case dropWhile isSpaceC s0 of
  ('[' : s1) -> case dropWhile isSpaceC s1 of
    (']' : s2) -> [([], s2)]
    _ -> readElems s1
  _ -> []
  where
    readElems s = [(x : xs, r2) | (x, r1) <- readsPrec 0 s, (xs, r2) <- readRest r1]
    readRest r = case dropWhile isSpaceC r of
      (',' : r') -> readElems r'
      (']' : r') -> [([], r')]
      _ -> []

instance Read Int where
  readsPrec _ s = readParen False (\t -> [(primIntegerToInt n, r) | (n, r) <- readIntegerPrefix t]) s

instance Read Integer where
  readsPrec _ s = readParen False readIntegerPrefix s

instance Read Double where
  readsPrec _ s0 = readParen False (\s -> [(primDoubleRead tok, r) | (tok, r) <- lexDouble (dropWhile isSpaceC s), not (null tok)]) s0

foreign import prim "doubleRead" primDoubleRead :: String -> Double

-- Single precision: values are doubles rounded to single precision after each operation.
foreign import prim "floatFromDouble" primFloatFromDouble :: Double -> Float
foreign import prim "floatToDouble" primFloatToDouble :: Float -> Double
foreign import prim "floatShow" primFloatShow :: Float -> String

lexDouble :: String -> [(String, String)]
lexDouble s =
  let (sign, s1) = case s of
        ('-' : rest) -> ("-", rest)
        _ -> ("", s)
      (ip, s2) = span isDigitC s1
      (fp, s3) = case s2 of
        ('.' : rest) | not (null ip) -> let (f, r) = span isDigitC rest in if null f then ("", s2) else ('.' : f, r)
        _ -> ("", s2)
      (ep, s4) = case s3 of
        (e : rest) | (e == 'e' || e == 'E') && not (null ip) ->
          let (sg, rest2) = case rest of
                ('-' : r) -> ("-", r)
                ('+' : r) -> ("", r)
                _ -> ("", rest)
              (ds, rest3) = span isDigitC rest2
          in if null ds then ("", s3) else ('e' : sg ++ ds, rest3)
        _ -> ("", s3)
  in if null ip then [] else [(sign ++ ip ++ fp ++ ep, s4)]

instance Read Char where
  readsPrec _ s = case dropWhile isSpaceC s of
    ('\'' : c : '\'' : rest) -> [(c, rest)]
    _ -> []
  readList s = case dropWhile isSpaceC s of
    ('"' : rest) -> readStrBody rest ""
    _ -> readListDefault s

readStrBody :: String -> String -> [(String, String)]
readStrBody ('"' : rest) acc = [(reverse acc, rest)]
readStrBody ('\\' : 'n' : rest) acc = readStrBody rest ('\n' : acc)
readStrBody ('\\' : 't' : rest) acc = readStrBody rest ('\t' : acc)
readStrBody ('\\' : '\\' : rest) acc = readStrBody rest ('\\' : acc)
readStrBody ('\\' : '"' : rest) acc = readStrBody rest ('"' : acc)
readStrBody (c : rest) acc = readStrBody rest (c : acc)
readStrBody [] _ = []

instance Read Bool where
  readsPrec _ s = readParen False (\r -> [(b, t) | (w, t) <- lex r, (b, n) <- [(True, "True"), (False, "False")], w == n]) s

lexWord :: String -> (String, String)
lexWord s = span isAlphaC s

isAlphaC :: Char -> Bool
isAlphaC c = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')

instance Read a => Read [a] where
  readsPrec _ s = readList s

instance (Read a, Read b) => Read (a, b) where
  readsPrec _ s0 = case dropWhile isSpaceC s0 of
    ('(' : s1) -> [((a, b), r4) | (a, r1) <- readsPrec 0 s1, (',' : r2) <- [dropWhile isSpaceC r1], (b, r3) <- readsPrec 0 r2, (')' : r4) <- [dropWhile isSpaceC r3]]
    _ -> []

instance (Read a, Read b, Read c) => Read (a, b, c) where
  readsPrec _ s0 = case dropWhile isSpaceC s0 of
    ('(' : s1) -> [((a, b, c), r6) | (a, r1) <- readsPrec 0 s1, (',' : r2) <- [dropWhile isSpaceC r1], (b, r3) <- readsPrec 0 r2, (',' : r4) <- [dropWhile isSpaceC r3], (c, r5) <- readsPrec 0 r4, (')' : r6) <- [dropWhile isSpaceC r5]]
    _ -> []

instance Read a => Read (Maybe a) where
  readsPrec d s =
    readParen False (\r -> [(Nothing, t) | ("Nothing", t) <- lex r]) s
      ++ readParen (d > 10) (\r -> [(Just x, t2) | ("Just", t1) <- lex r, (x, t2) <- readsPrec 11 t1]) s

readIntegerPrefix :: String -> [(Integer, String)]
readIntegerPrefix s0 =
  let s = dropWhile isSpaceC s0
  in case s of
       ('-' : rest) -> [(negate n, r) | (n, r) <- digitsPrefix rest]
       _ -> digitsPrefix s

digitsPrefix :: String -> [(Integer, String)]
digitsPrefix s = case span isDigitC s of
  ([], _) -> []
  (ds, rest) -> [(foldl (\acc d -> acc * 10 + primIntToInteger (ord d - 48)) 0 ds, rest)]

-- ----- Float --------------------------------------------------------------------------

floatOp :: (Double -> Double -> Double) -> Float -> Float -> Float
floatOp f x y = primFloatFromDouble (f (primFloatToDouble x) (primFloatToDouble y))

instance Eq Float where
  x == y = primFloatToDouble x == primFloatToDouble y

instance Ord Float where
  x < y = primFloatToDouble x < primFloatToDouble y
  x <= y = primFloatToDouble x <= primFloatToDouble y
  x > y = primFloatToDouble y < primFloatToDouble x
  x >= y = primFloatToDouble y <= primFloatToDouble x

instance Show Float where
  showsPrec p x r = if primFloatToDouble x < 0 && p > 6
    then '(' : primFloatShow x ++ (')' : r)
    else primFloatShow x ++ r

instance Num Float where
  x + y = floatOp (+) x y
  x - y = floatOp (-) x y
  x * y = floatOp (*) x y
  negate x = primFloatFromDouble (negate (primFloatToDouble x))
  abs x = primFloatFromDouble (abs (primFloatToDouble x))
  signum x = primFloatFromDouble (signum (primFloatToDouble x))
  fromInteger n = primFloatFromDouble (fromInteger n)

instance Real Float where
  toRational x = toRational (primFloatToDouble x)

instance Fractional Float where
  x / y = floatOp (/) x y
  fromRational r = primFloatFromDouble (fromRational r)

instance Floating Float where
  pi = primFloatFromDouble pi
  exp x = primFloatFromDouble (exp (primFloatToDouble x))
  log x = primFloatFromDouble (log (primFloatToDouble x))
  sqrt x = primFloatFromDouble (sqrt (primFloatToDouble x))
  sin x = primFloatFromDouble (sin (primFloatToDouble x))
  cos x = primFloatFromDouble (cos (primFloatToDouble x))
  tan x = primFloatFromDouble (tan (primFloatToDouble x))
  x ** y = floatOp (**) x y

instance RealFrac Float where
  properFraction x = let (n, f) = properFraction (primFloatToDouble x) in (n, primFloatFromDouble f)
  truncate x = truncate (primFloatToDouble x)
  round x = round (primFloatToDouble x)
  floor x = floor (primFloatToDouble x)
  ceiling x = ceiling (primFloatToDouble x)

instance Enum Float where
  toEnum n = primFloatFromDouble (primIntToDouble n)
  fromEnum x = fromEnum (primFloatToDouble x)
  enumFrom x = map primFloatFromDouble (enumFrom (primFloatToDouble x))
  enumFromTo x y = map primFloatFromDouble (enumFromTo (primFloatToDouble x) (primFloatToDouble y))
  enumFromThen x y = map primFloatFromDouble (enumFromThen (primFloatToDouble x) (primFloatToDouble y))
  enumFromThenTo x y z = map primFloatFromDouble (enumFromThenTo (primFloatToDouble x) (primFloatToDouble y) (primFloatToDouble z))

instance Read Float where
  readsPrec d s = [(primFloatFromDouble x, r) | (x, r) <- readsPrec d s]

-- ----- Enum Double -------------------------------------------------------------------

instance Enum Double where
  succ x = x + 1
  pred x = x - 1
  toEnum n = primIntToDouble n
  fromEnum x = primIntegerToInt (primDoubleTruncate x)
  enumFrom x = numericEnumFrom x
  enumFromTo x y = takeWhile (<= y + 0.5) (numericEnumFrom x)
  enumFromThen x y = numericEnumFromThen x y
  enumFromThenTo x y z =
    let d = y - x
        p = if y >= x then (<= z + d / 2) else (>= z + d / 2)
    in takeWhile p (numericEnumFromThen x y)

-- GHC computes the k-th element as x + k * step, not by repeated addition.
numericEnumFrom :: Double -> [Double]
numericEnumFrom n = go 0
  where go k = (n + k) : go (k + 1)

numericEnumFromThen :: Double -> Double -> [Double]
numericEnumFromThen n m = go 0
  where
    step = m - n
    go k = (n + k * step) : go (k + 1)

-- ----- Traversable -------------------------------------------------------------------

-- ----- more tuples -------------------------------------------------------------------

instance (Show a, Show b, Show c, Show d) => Show (a, b, c, d) where
  showsPrec _ (a, b, c, d) = showChar '(' . shows a . showChar ',' . shows b . showChar ',' . shows c . showChar ',' . shows d . showChar ')'

instance (Show a, Show b, Show c, Show d, Show e) => Show (a, b, c, d, e) where
  showsPrec _ (a, b, c, d, e) = showChar '(' . shows a . showChar ',' . shows b . showChar ',' . shows c . showChar ',' . shows d . showChar ',' . shows e . showChar ')'

instance (Show a, Show b, Show c, Show d, Show e, Show f) => Show (a, b, c, d, e, f) where
  showsPrec _ (a, b, c, d, e, f) = showChar '(' . shows a . showChar ',' . shows b . showChar ',' . shows c . showChar ',' . shows d . showChar ',' . shows e . showChar ',' . shows f . showChar ')'

instance (Show a, Show b, Show c, Show d, Show e, Show f, Show g) => Show (a, b, c, d, e, f, g) where
  showsPrec _ (a, b, c, d, e, f, g) = showChar '(' . shows a . showChar ',' . shows b . showChar ',' . shows c . showChar ',' . shows d . showChar ',' . shows e . showChar ',' . shows f . showChar ',' . shows g . showChar ')'

instance (Eq a, Eq b, Eq c, Eq d) => Eq (a, b, c, d) where
  (a, b, c, d) == (e, f, g, h) = a == e && b == f && c == g && d == h

instance (Ord a, Ord b, Ord c, Ord d) => Ord (a, b, c, d) where
  compare (a, b, c, d) (e, f, g, h) = case compare a e of
    EQ -> case compare b f of
      EQ -> case compare c g of
        EQ -> compare d h
        other -> other
      other -> other
    other -> other

-- ----- misc --------------------------------------------------------------------------

even' :: Int -> Bool
even' n = even n

const' :: a -> b -> a
const' = const
